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

  Roo::Calamine::Native::Reader.singleton_class.prepend(Module.new do
    define_method(:new) do |*args|
      Oracle.counts[:native_readers] += 1
      super(*args)
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

    state = cell.instance_variables.sort.map { |ivar| [ivar, describe_value(cell.instance_variable_get(ivar))] }
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

  # The Roo::Base API on top of the cells: bounds, rows, columns, formulas.
  def describe_api(workbook)
    workbook.sheets.map do |sheet|
      workbook.default_sheet = sheet
      bounds = BOUNDS.map { |m| workbook.public_send(m) }
      rows = bounds[0] ? (bounds[0]..bounds[1]).map { |i| workbook.row(i).map { |v| describe_value(v) } } : []
      columns = bounds[2] ? (bounds[2]..bounds[3]).map { |i| workbook.column(i).map { |v| describe_value(v) } } : []
      [sheet, bounds, rows, columns, workbook.formulas(sheet), outcome { workbook.to_csv }]
    end
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
