//! The roo cells themselves. Under a roo version whose constructors it knows,
//! [`Converter::cell`] allocates each cell and sets the instance variables
//! its constructor sets, in the same order and with the same values;
//! otherwise, and for any value it does not compute exactly the way roo
//! does, it calls roo's constructor with the same arguments.

use magnus::rb_sys::{AsRawId, AsRawValue, FromRawValue};
use magnus::value::{IntoId, Lazy, LazyId, ReprValue};
use magnus::{prelude::*, Error, IntoValue, RArray, RClass, RObject, RString, Ruby, Value};

use super::{
    Converter, NumberFormat, ValueType, CELL_BOOLEAN, CELL_DATE, CELL_DATETIME, CELL_EMPTY,
    CELL_NUMBER, CELL_STRING, CELL_TIME, COORDINATE, NUMERIC_OR_FORMULA,
};
use crate::numeric::{create_numeric, leading_integer};
use crate::reader::{Body, Kind, Mode, RawCell, Reference};

static TIME: Lazy<RClass> = Lazy::new(|r| r.class_time());
static LINK: Lazy<RClass> = Lazy::new(|r| r.eval("Roo::Link").expect("roo defines Roo::Link"));

static CELL_VALUE: LazyId = LazyId::new("@cell_value");
static CELL_TYPE: LazyId = LazyId::new("@cell_type");
static FORMULA: LazyId = LazyId::new("@formula");
static STYLE: LazyId = LazyId::new("@style");
static COORDINATE_IVAR: LazyId = LazyId::new("@coordinate");
static VALUE: LazyId = LazyId::new("@value");
static FORMAT: LazyId = LazyId::new("@format");
static DATETIME: LazyId = LazyId::new("@datetime");

const SECONDS_IN_DAY: f64 = 86_400.0;

/// The roo cell a value cell becomes, with what its value is computed from.
#[derive(Clone, Copy)]
enum NumericCell {
    Number(NumberFormat),
    /// `Cell::Date` from the cell's text.
    DateFromText,
    /// `Cell::Date`, `Cell::DateTime`, and `Cell::Time` from the text's
    /// `to_f`.
    DateFromNumber(f64),
    DateTime(f64),
    Time(f64),
}

impl NumericCell {
    fn temporal(number: f64) -> Self {
        if number < 1.0 {
            Self::Time(number)
        } else if (number - number.floor()).abs() > 0.000001 {
            Self::DateTime(number)
        } else {
            Self::DateFromNumber(number)
        }
    }
}

struct Common {
    formula: Option<RString>,
    style: Value,
    link: Value,
    coordinate: Value,
}

impl Converter {
    /// `Coordinate.new(row, column)`.
    pub(super) fn new_coordinate<T: IntoValue>(
        &self,
        ruby: &Ruby,
        row: T,
        column: T,
    ) -> Result<Value, Error> {
        let class = ruby.get_inner(&COORDINATE);
        if !self.build_cells {
            return class.new_instance((row, column));
        }
        let row = row.into_value_with(ruby);
        let column = column.into_value_with(ruby);
        let coordinate = magnus::rb_sys::protect(|| unsafe {
            let coordinate = rb_sys::rb_obj_alloc(class.as_raw());
            rb_sys::rb_ary_push(coordinate, row.as_raw());
            rb_sys::rb_ary_push(coordinate, column.as_raw());
            rb_sys::rb_obj_freeze(coordinate)
        })?;
        Ok(unsafe { Value::from_raw(coordinate) })
    }

    /// Builds the cell `SheetDoc#cell_from_xml` builds for `raw`. `None` means
    /// roo stores no cell (an empty cell without the `empty_cell` option).
    /// `known` is the value [`Self::known_value`] returned for `raw`, which
    /// the cell then holds.
    pub fn cell(
        &self,
        ruby: &Ruby,
        buffer: &str,
        raw: &RawCell,
        row_number: usize,
        column: usize,
        known: Option<Value>,
    ) -> Result<Option<(Value, Value)>, Error> {
        let coordinate = self.coordinate(ruby, raw, row_number, column)?;
        let style = self.style(ruby, raw)?;
        let common = Common {
            formula: raw
                .formula
                .as_ref()
                .map(|f| ruby.str_new(&buffer[f.clone()])),
            style,
            link: self.link(ruby, coordinate),
            coordinate,
        };
        let cell = match &raw.body {
            Body::Empty => {
                if !self.empty_cell {
                    return Ok(None);
                }
                if self.build_cells {
                    let cell = Self::allocate(ruby, &CELL_EMPTY)?;
                    cell.ivar_set(*COORDINATE_IVAR, coordinate)?;
                    cell.as_value()
                } else {
                    ruby.get_inner(&CELL_EMPTY).new_instance((coordinate,))?
                }
            }
            Body::Inline(text) => {
                let value = known
                    .filter(|_| !common.link.to_bool())
                    .unwrap_or_else(|| ruby.str_new(&buffer[text.clone()]).as_value());
                self.string(ruby, value, &common, known)?
            }
            // roo looks up the style format for every `v` cell, which raises when
            // the workbook has no styles part, so this does too.
            Body::Value(text) => {
                let text = &buffer[text.clone()];
                match (raw.kind, self.style_info(ruby, style)?) {
                    (Kind::Shared, _) => {
                        let value = self.shared_string(ruby, text)?;
                        self.string(ruby, value, &common, known)?
                    }
                    (Kind::Boolean, _) => self.boolean(ruby, text, &common)?,
                    (Kind::Str, _) => {
                        let value = known
                            .filter(|_| !common.link.to_bool())
                            .unwrap_or_else(|| ruby.str_new(text).as_value());
                        self.string(ruby, value, &common, known)?
                    }
                    (kind, (format, format_type, number_format)) => {
                        let value_type = if kind == Kind::InlineStr {
                            ValueType::Number
                        } else {
                            format_type
                        };
                        let read = (value_type, number_format);
                        self.numeric(ruby, text, format, read, known, &common)?
                    }
                }
            }
        };
        Ok(Some((coordinate, cell)))
    }

    /// The `value` of the cell [`Self::cell`] builds for `raw` without its
    /// constructor, without building it. `None` when that cell would come
    /// from its constructor.
    pub fn known_value(
        &self,
        ruby: &Ruby,
        buffer: &str,
        raw: &RawCell,
    ) -> Result<Option<Value>, Error> {
        let Reference::At { row, column } = raw.reference else {
            return Ok(None);
        };
        if !self.build_cells {
            return Ok(None);
        }
        let link = self.link_at(ruby, row, column);
        if link.to_bool() {
            let value = match (&raw.body, raw.kind) {
                (Body::Inline(text), _) | (Body::Value(text), Kind::Str) => {
                    ruby.str_new(&buffer[text.clone()]).as_value()
                }
                _ => return Ok(None),
            };
            return ruby.get_inner(&LINK).new_instance((link, value)).map(Some);
        }
        let text = match &raw.body {
            Body::Empty => return Ok(Some(ruby.qnil().as_value())),
            Body::Inline(text) => return Ok(Some(ruby.str_new(&buffer[text.clone()]).as_value())),
            Body::Value(text) => &buffer[text.clone()],
        };
        let (_, format_type, number_format) = self.style_info(ruby, self.style(ruby, raw)?)?;
        let numeric = match (raw.kind, format_type) {
            (Kind::Shared, _) => return Ok(Some(self.shared_string(ruby, text)?)),
            (Kind::Boolean, _) => {
                let value = match text {
                    "0" => Some(false),
                    "1" => Some(true),
                    _ => None,
                };
                return Ok(value.map(|value| value.into_value_with(ruby)));
            }
            (Kind::Str, _) => return Ok(Some(ruby.str_new(text).as_value())),
            (Kind::InlineStr, _) | (Kind::ByFormat, ValueType::Number) => {
                NumericCell::Number(number_format)
            }
            (Kind::ByFormat, ValueType::Date) => NumericCell::DateFromText,
            (Kind::ByFormat, ValueType::DateTime | ValueType::Time) => {
                NumericCell::temporal(ruby.str_new(text).funcall("to_f", ())?)
            }
        };
        Ok(self.computed(ruby, numeric, text)?.map(|(value, _)| value))
    }

    /// Non-text cells with a truthy link keep roo's link constructors.
    fn builds(&self, common: &Common) -> bool {
        self.build_cells && !common.link.to_bool()
    }

    fn allocate(ruby: &Ruby, class: &Lazy<RClass>) -> Result<RObject, Error> {
        let cell = ruby.get_inner(class).obj_alloc()?;
        Ok(RObject::from_value(cell).expect("roo's cells are plain objects"))
    }

    /// Allocates a cell and sets what `Cell::Base#initialize` sets, in its
    /// order, with `value` as the final `@value`, then `rest`.
    fn build_cell(
        ruby: &Ruby,
        class: &Lazy<RClass>,
        cell_value: Value,
        cell_type: Option<RArray>,
        common: &Common,
        value: Value,
        rest: &[(&LazyId, Value)],
    ) -> Result<Value, Error> {
        let class = ruby.get_inner(class);
        let fields = [
            Some((*CELL_VALUE, cell_value)),
            cell_type.map(|value| (*CELL_TYPE, value.as_value())),
            common.formula.map(|value| (*FORMULA, value.as_value())),
            (!matches!(i64::try_convert(common.style), Ok(1))).then(|| (*STYLE, common.style)),
            Some((*COORDINATE_IVAR, common.coordinate)),
            Some((*VALUE, value)),
        ];
        let cell = magnus::rb_sys::protect(|| unsafe {
            let cell = rb_sys::rb_obj_alloc(class.as_raw());
            for (name, value) in fields.into_iter().flatten() {
                rb_sys::rb_ivar_set(cell, name.into_id_with(ruby).as_raw(), value.as_raw());
            }
            for (name, value) in rest {
                rb_sys::rb_ivar_set(cell, (***name).into_id_with(ruby).as_raw(), value.as_raw());
            }
            cell
        })?;
        Ok(unsafe { Value::from_raw(cell) })
    }

    /// `Cell::String.new(value, formula, style, link, coordinate)`.
    fn string(
        &self,
        ruby: &Ruby,
        value: Value,
        common: &Common,
        known: Option<Value>,
    ) -> Result<Value, Error> {
        if self.build_cells {
            let result = if !common.link.to_bool() {
                value
            } else {
                match known {
                    Some(link) => link,
                    None => ruby.get_inner(&LINK).new_instance((common.link, value))?,
                }
            };
            return Self::build_cell(ruby, &CELL_STRING, value, None, common, result, &[]);
        }
        let Common {
            formula,
            style,
            link,
            coordinate,
        } = *common;
        ruby.get_inner(&CELL_STRING)
            .new_instance((value, formula, style, link, coordinate))
    }

    /// `Cell::Boolean.new(text, formula, style, link, coordinate)`.
    fn boolean(&self, ruby: &Ruby, text: &str, common: &Common) -> Result<Value, Error> {
        let content = ruby.str_new(text).as_value();
        // `create_boolean` is `text.to_i == 1`.
        let value = match text {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        };
        if let Some(value) = value.filter(|_| self.builds(common)) {
            let value = value.into_value_with(ruby);
            return Self::build_cell(ruby, &CELL_BOOLEAN, content, None, common, value, &[]);
        }
        let Common {
            formula,
            style,
            link,
            coordinate,
        } = *common;
        ruby.get_inner(&CELL_BOOLEAN)
            .new_instance((content, formula, style, link, coordinate))
    }

    /// The `Cell::Number`, `Cell::Date`, `Cell::DateTime`, or `Cell::Time`
    /// for a value cell.
    fn numeric(
        &self,
        ruby: &Ruby,
        text: &str,
        format: Value,
        (value_type, number_format): (ValueType, NumberFormat),
        known: Option<Value>,
        common: &Common,
    ) -> Result<Value, Error> {
        let content = ruby.str_new(text);
        let numeric = match value_type {
            ValueType::Number => NumericCell::Number(number_format),
            ValueType::Date => NumericCell::DateFromText,
            ValueType::DateTime | ValueType::Time => {
                NumericCell::temporal(content.funcall("to_f", ())?)
            }
        };
        let (class, cell_value, base) = match numeric {
            NumericCell::Number(_) => (&CELL_NUMBER, content.as_value(), None),
            NumericCell::DateFromText => (&CELL_DATE, content.as_value(), Some(self.base_date)),
            NumericCell::DateFromNumber(number) => (
                &CELL_DATE,
                ruby.float_from_f64(number).as_value(),
                Some(self.base_date),
            ),
            NumericCell::DateTime(number) => (
                &CELL_DATETIME,
                ruby.float_from_f64(number).as_value(),
                Some(self.base_timestamp),
            ),
            NumericCell::Time(number) => (
                &CELL_TIME,
                ruby.float_from_f64(number).as_value(),
                Some(self.base_timestamp),
            ),
        };
        let numeric_or_formula = ruby.get_inner(&NUMERIC_OR_FORMULA).as_value();
        let excelx_type = ruby.ary_new_from_values(&[numeric_or_formula, format]);
        if self.builds(common) {
            let cell_type = Some(excelx_type);
            let computed = match (known, numeric) {
                (Some(value), NumericCell::Time(_)) => self
                    .computed(ruby, numeric, text)?
                    .map(|(_, datetime)| (value, datetime)),
                (Some(value), _) => Some((value, None)),
                (None, _) => self.computed(ruby, numeric, text)?,
            };
            match computed {
                Some((value, None)) => {
                    let rest = [(&FORMAT, format)];
                    return Self::build_cell(
                        ruby, class, cell_value, cell_type, common, value, &rest,
                    );
                }
                Some((value, Some(datetime))) => {
                    let rest = [(&FORMAT, format), (&DATETIME, datetime)];
                    return Self::build_cell(
                        ruby, class, cell_value, cell_type, common, value, &rest,
                    );
                }
                None => {}
            }
        }
        let Common {
            formula,
            style,
            link,
            coordinate,
        } = *common;
        let class = ruby.get_inner(class);
        match base {
            None => {
                let cell = class.new_instance((
                    cell_value,
                    formula,
                    excelx_type,
                    style,
                    link,
                    coordinate,
                ))?;
                if link.is_nil() {
                    self.keep_number_as_read(ruby, style, format, content, cell)?;
                }
                Ok(cell)
            }
            Some(base) => class.new_instance((
                cell_value,
                formula,
                excelx_type,
                style,
                link,
                ruby.get_inner(base),
                coordinate,
            )),
        }
    }

    /// Document mode: roo builds every cell before a caller can change a
    /// format string in place, and `Cell::Number#create_numeric` reads it,
    /// so a cell built later by its constructor gets the value the format
    /// first read gives.
    fn keep_number_as_read(
        &self,
        ruby: &Ruby,
        style: Value,
        format: Value,
        content: RString,
        cell: Value,
    ) -> Result<(), Error> {
        if self.mode != Mode::Document {
            return Ok(());
        }
        let Some(info) = self.style_as_read(style) else {
            return Ok(());
        };
        if info.read_from(format) {
            return Ok(());
        }
        let as_read = ruby.enc_str_new(&info.format_text, info.format_encoding);
        as_read.freeze();
        let excelx_type = ruby.ary_new_from_values(&[
            ruby.get_inner(&NUMERIC_OR_FORMULA).as_value(),
            as_read.as_value(),
        ]);
        let value: Value = ruby
            .get_inner(&CELL_NUMBER)
            .new_instance((
                content,
                ruby.qnil(),
                excelx_type,
                style,
                ruby.qnil(),
                ruby.qnil(),
            ))?
            .funcall("value", ())?;
        cell.funcall::<_, _, Value>("value=", (value,))?;
        Ok(())
    }

    /// The `@value` roo's constructor computes, plus the `@datetime` of a
    /// `Cell::Time`, or `None` when they cannot be computed the way roo does.
    fn computed(
        &self,
        ruby: &Ruby,
        numeric: NumericCell,
        text: &str,
    ) -> Result<Option<(Value, Option<Value>)>, Error> {
        let base_date = ruby.get_inner(self.base_date);
        Ok(match numeric {
            NumericCell::Number(number_format) => {
                let number = match number_format {
                    NumberFormat::Float => create_numeric(text, true),
                    NumberFormat::ByText => create_numeric(text, false),
                    NumberFormat::Unmatchable => None,
                };
                number.map(|number| (number.into_value_with(ruby), None))
            }
            // `base_date + value.to_i`
            NumericCell::DateFromText => match leading_integer(text) {
                Some(days) => Some((base_date.funcall("+", (days,))?, None)),
                None => None,
            },
            NumericCell::DateFromNumber(number) => match to_i64(number.trunc()) {
                Some(days) => Some((base_date.funcall("+", (days,))?, None)),
                None => None,
            },
            NumericCell::DateTime(number) => {
                self.datetime(ruby, number)?.map(|value| (value, None))
            }
            // `(value.to_f * 86_400).round.to_i`
            NumericCell::Time(number) => match (
                to_i64((number * SECONDS_IN_DAY).round()),
                self.datetime(ruby, number)?,
            ) {
                (Some(seconds), Some(datetime)) => {
                    Some((ruby.integer_from_i64(seconds).as_value(), Some(datetime)))
                }
                _ => None,
            },
        })
    }

    /// `Cell::DateTime#create_datetime(base_timestamp, number)`:
    /// `Time.at((base_timestamp + number.round(6) * 86_400).round(0)).utc.to_datetime`.
    fn datetime(&self, ruby: &Ruby, number: f64) -> Result<Option<Value>, Error> {
        let Some(base_seconds) = self.base_seconds else {
            return Ok(None);
        };
        let rounded: f64 = ruby.float_from_f64(number).funcall("round", (6,))?;
        let Some(timestamp) = to_i64((base_seconds as f64 + rounded * SECONDS_IN_DAY).round())
        else {
            return Ok(None);
        };
        let time: Value = ruby.get_inner(&TIME).funcall("at", (timestamp,))?;
        let utc: Value = time.funcall("utc", ())?;
        Ok(Some(utc.funcall("to_datetime", ())?))
    }
}

fn to_i64(integral: f64) -> Option<i64> {
    (integral.abs() < 9.0e18).then_some(integral as i64)
}
