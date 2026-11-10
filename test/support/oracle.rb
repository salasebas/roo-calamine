# frozen_string_literal: true

# roo is the oracle: these helpers read a workbook once with roo's own reader
# and once with roo-calamine, and describe everything roo exposes about the
# result, so a test can compare the two descriptions.
module Oracle
  @counts = Hash.new(0)

  class << self
    attr_reader :counts
  end

  Roo::Excelx::SheetDoc.prepend(Module.new do
    define_method(:cell_from_xml) do |*args|
      Oracle.counts[:roo_cells] += 1
      super(*args)
    end
  end)

  Roo::Excelx::Sheet.prepend(Module.new do
    define_method(:row) do |*args|
      Oracle.counts[:sheet_rows] += 1
      super(*args)
    end
  end)

  [Roo::Excelx::Coordinate, *%i[Number Date DateTime Time String Boolean Empty].map { Roo::Excelx::Cell.const_get(_1) }]
    .each do |klass|
      klass.prepend(Module.new do
        define_method(:initialize) do |*args|
          Oracle.counts[:constructors] += 1 if instance_of?(klass)
          super(*args)
        end
      end)
    end

  Roo::Calamine::Native::Reader.singleton_class.prepend(Module.new do
    define_method(:new) do |*args|
      Oracle.counts[:native_readers] += 1
      super(*args)
    end
  end)

  Roo::Calamine::Native::Store.singleton_class.prepend(Module.new do
    define_method(:read) do |*args|
      Oracle.counts[:native_readers] += 1
      super(*args).tap { |store| Oracle.counts[:lazy_stores] += 1 if store }
    end
  end)

  Roo::Calamine::Native.singleton_class.prepend(Module.new do
    define_method(:shared_strings) do |*args|
      super(*args).tap { Oracle.counts[:native_shared_strings] += 1 }
    end
  end)

  BOUNDS = %i[first_row last_row first_column last_column].freeze
  STREAM_OPTIONS = [{}, { pad_cells: true }, { offset: 1 }, { max_rows: 2 },
                    { offset: 1, max_rows: 3, pad_cells: true }].freeze

  module_function

  # Runs the block with the native reader on or off and returns its result,
  # or the error it raised, plus how many cells each reader parsed.
  def read(native:, &)
    Roo::Calamine.enabled = native
    counts.clear
    [outcome(&), counts.dup]
  ensure
    Roo::Calamine.enabled = true
  end

  def outcome
    [:ok, yield]
  rescue StandardError => e
    # roo extracts each workbook into a new temporary directory, named in
    # some messages.
    [:error, e.class, e.message.gsub(%r{\S*/roo_[^/\s]*\d{8}-\d+-\w+}, "<tmpdir>")]
  end

  def describe_value(value)
    described = [value.class, value.frozen?, value]
    described << value.href if value.is_a?(Roo::Link)
    described
  end

  def describe_cell(cell)
    return cell if cell.nil?

    state = cell.instance_variables.map { |ivar| [ivar, describe_value(cell.instance_variable_get(ivar))] }
    formatted = outcome { silence { cell.formatted_value } }
    [cell.class, describe_value(cell.value), cell.type, cell.empty?, formatted, state]
  end

  def describe_cells(workbook)
    workbook.sheets.map do |sheet|
      cells = workbook.sheet_for(sheet).cells
      [sheet, cells.map { |key, cell| [key, describe_cell(cell)] }]
    end
  end

  def describe_stream(workbook, options)
    workbook.sheets.map do |sheet|
      rows = workbook.each_row_streaming(options.merge(sheet:)).map do |row|
        row.map { |cell| describe_cell(cell) }
      end
      [sheet, rows]
    end
  end

  # What Roo::Calamine.each_row_values must yield: the value of each cell
  # that roo's each_row_streaming(pad_cells: true) yields.
  def describe_streamed_values(workbook)
    workbook.sheets.map do |sheet|
      rows = workbook.each_row_streaming(sheet:, pad_cells: true).map do |row|
        row.map { |cell| describe_value(cell&.value) }
      end
      [sheet, rows]
    end
  end

  def describe_each_row_values(path, options)
    Roo::Excelx.new(path, options.dup).sheets.map do |sheet|
      rows = Roo::Calamine.each_row_values(path, sheet:, **options).map do |values|
        values.map { |value| describe_value(value) }
      end
      [sheet, rows]
    end
  end

  # roo's two shared-string arrays, read in `order`, with the encoding of
  # each string and whether every string is an object of its own.
  def describe_shared_strings(workbook, order)
    strings = workbook.shared_strings
    arrays = order.map { |method| outcome { strings.public_send(method) } }
    objects = arrays.filter_map { |status, value| value if status == :ok }.flatten(1)
    described = arrays.map do |status, *rest|
      status == :ok ? [status, rest.first.map { |string| [*describe_value(string), string.encoding] }] : [status, *rest]
    end
    [order, described, objects.map(&:object_id).uniq.size == objects.size]
  end

  # The Roo::Base API on top of the cells: bounds, rows, columns, formulas,
  # and #parse and #each under every option set.
  def describe_api(workbook)
    workbook.sheets.map do |sheet|
      workbook.default_sheet = sheet
      bounds = BOUNDS.map { |m| workbook.public_send(m) }
      rows = bounds[0] ? (bounds[0]..bounds[1]).map { |i| workbook.row(i).map { |v| describe_value(v) } } : []
      columns = bounds[2] ? (bounds[2]..bounds[3]).map { |i| workbook.column(i).map { |v| describe_value(v) } } : []
      [sheet, bounds, rows, columns, workbook.formulas(sheet), outcome { workbook.to_csv },
       describe_parse(workbook, bounds[0])]
    end
  end

  # Option sets for Roo::Base#each and #parse. The ones that look for a header
  # row look for `header`, a value of the sheet's first row (nil when the
  # sheet is empty). :clean changes the cells, so those sets come last.
  def parse_options(header)
    [{}, { headers: true }, { headers: :first_row }, { header_search: [header] }, { first: header },
     { first: /#{Regexp.escape(header.to_s)}/ }, { first: header, missing: "no such header" },
     { headers: true, clean: true }, { clean: true }]
  end

  # #parse without a block, #parse with one, and #each, which yield the same
  # rows through different code.
  def describe_parse(workbook, first_row)
    header = first_row && workbook.row(first_row).compact.first
    parse_options(header).map do |options|
      [options,
       outcome { describe_rows(workbook.parse(options.dup)) },
       outcome { describe_rows(workbook.parse(options.dup) { |row| row }) },
       outcome { describe_rows(workbook.each(options.dup).to_a) },
       workbook.header_line, workbook.headers]
    end
  end

  def describe_rows(rows)
    rows.map do |row|
      row.is_a?(Hash) ? row.map { |key, value| [describe_value(key), describe_value(value)] } : row.map { |v| describe_value(v) }
    end
  end

  LOOKUPS = %i[cell celltype formula excelx_type excelx_value formatted_value excelx_format empty?].freeze

  # Roo::Excelx's single-cell lookups over the bounds and one cell around
  # them, before anything builds `cells`, then whether `cells`, built after
  # them, holds the very values the lookups returned. Each step is described
  # on its own, so an error in one does not hide what the others return.
  def describe_lookups(workbook)
    workbook.sheets.map do |sheet|
      bounds = outcome { BOUNDS.map { |m| workbook.public_send(m, sheet) } }
      values = {}
      lookups = keys_around(bounds).map do |key|
        values[key] = outcome { workbook.cell(*key, sheet) }.last
        [key, describe_lookup(workbook, sheet, *key)]
      end
      kept = outcome do
        cells = workbook.sheet_for(sheet).cells
        [values.map { |key, value| value.equal?(cells[key]&.value) }, cells.count { |key, cell| !key.equal?(cell.coordinate) }]
      end
      [sheet, bounds, lookups, kept]
    end
  end

  # #each and #parse without a block after one single-cell lookup, before
  # anything builds `cells`, then whether `cells`, built after them, holds
  # the very values #each and #parse(headers: true) returned. The other
  # parses look for the first row's last value, so their columns need not
  # start at the sheet's first.
  def describe_row_reads(workbook)
    workbook.sheets.map do |sheet|
      workbook.default_sheet = sheet
      bounds = BOUNDS.map { |m| workbook.public_send(m) }
      workbook.cell(bounds[0], bounds[2]) if bounds[0]
      searched = describe_header_parses(workbook, bounds[0] && workbook.row(bounds[0]).compact.last)
      workbook.header_line = 1
      rows = outcome { workbook.each.to_a }
      parsed = outcome { workbook.parse(headers: true) }
      kept = outcome { kept_row_values(workbook.sheet_for(sheet).cells, workbook, bounds[2], rows, parsed) }
      [sheet, bounds, searched, described_rows(rows), described_rows(parsed), kept]
    end
  end

  def described_rows(read)
    read.first == :ok ? [:ok, describe_rows(read.last)] : read.first
  end

  def describe_header_parses(workbook, header)
    [{ header_search: [header] }, { last: header }].map { |options| outcome { describe_rows(workbook.parse(options)) } }
  end

  def kept_row_values(cells, workbook, first_column, rows, parsed)
    held = ->(value, line, column) { value.equal?(cells[[line, column]]&.value) }
    [kept_each_values(held, first_column, rows), kept_parse_values(held, workbook, parsed)].map(&:tally)
  end

  def kept_each_values(held, first_column, rows)
    return [] unless rows.first == :ok

    rows.last.each_with_index.flat_map do |row, i|
      row.each_with_index.map { |value, j| held.call(value, i + 1, first_column + j) }
    end
  end

  # parse(headers: true) keeps, for a repeated header, the last column's value.
  def kept_parse_values(held, workbook, parsed)
    return [] unless parsed.first == :ok

    columns = workbook.headers.to_h
    parsed.last.each_with_index.flat_map do |row, i|
      row.map { |key, value| held.call(value, workbook.header_line + i, columns[key]) }
    end
  end

  # Every [row, column] from one before the bounds to one after them.
  def keys_around(bounds)
    first_row, last_row, first_column, last_column = bounds.first == :ok && bounds.last[0] ? bounds.last : [1, 1, 1, 1]
    ((first_row - 1)..(last_row + 1)).to_a.product(((first_column - 1)..(last_column + 1)).to_a)
  end

  def describe_lookup(workbook, sheet, row, column)
    described = LOOKUPS.map { |m| outcome { silence { describe_value(workbook.public_send(m, row, column, sheet)) } } }
    described << outcome { workbook.font(row, column, sheet)&.then { [_1.bold?, _1.italic?, _1.underline?] } }
  end

  NULL = File.open(File::NULL, "w")

  def silence
    original = $stderr
    $stderr = NULL
    yield
  ensure
    $stderr = original
  end
end
