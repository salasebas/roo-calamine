# frozen_string_literal: true

require_relative "test_helper"

# Roo::Excelx#to_csv writes the bytes roo writes, under every way of calling
# it, for every cell type, and up to the cell where roo raises.
class ToCsvTest < Minitest::Test
  include OracleAssertions

  @roo_fields = 0
  @native_writes = 0

  class << self
    attr_accessor :roo_fields, :native_writes
  end

  Roo::Formatters::CSV.prepend(Module.new do
    define_method(:cell_to_csv) do |*args|
      ToCsvTest.roo_fields += 1
      super(*args)
    end
  end)

  Roo::Calamine::Native::Store.prepend(Module.new do
    define_method(:write_csv) do |*args|
      ToCsvTest.native_writes += 1
      super(*args)
    end
  end)

  HYPERLINK_RELS = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/?q=&quot;x&quot;,y" TargetMode="External"/></Relationships>).freeze

  # Rows 1 and 3 and column A are blank, so roo writes them before the bounds.
  TYPES = <<~XML
    <row r="2"><c r="B2" t="str"><v>say "hi", ok</v></c><c r="C2" t="inlineStr"><is><t>a;b&#9;c&#10;d ☺</t></is></c>
      <c r="D2" t="str"><v></v></c><c r="E2"><v>12</v></c><c r="F2"><v>1.5</v></c><c r="G2"><v>2.0</v></c>
      <c r="H2"><v>1E3</v></c><c r="I2"><v>0.30000000000000004</v></c><c r="J2"><v>12345678901234567890</v></c>
      <c r="K2" s="4"><v>0.25</v></c><c r="L2" s="5"><v>3</v></c><c r="M2"><v>-0</v></c></row>
    <row r="4"><c r="B4" s="1"><v>45000</v></c><c r="C4" s="2"><v>45000.5</v></c><c r="D4" s="3"><v>0.25</v></c>
      <c r="E4" s="3"><v>1.5</v></c><c r="F4" t="b"><v>1</v></c><c r="G4" t="b"><v>0</v></c><c r="H4" t="e"><v>#DIV/0!</v></c>
      <c r="I4" s="6"><v>45000.75</v></c><c r="J4"/><c r="K4" t="s"><v>0</v></c><c r="O4" t="str"><v></v></c></row>
    <row r="5"><c r="B5"><f>1+1</f><v>2</v></c><c r="C5"><f>5/2</f><v>2.5</v></c><c r="D5"><f>4/2</f><v>2.0</v></c>
      <c r="E5" t="str"><f>"a"&amp;"""b"</f><v>a"b</v></c><c r="F5" t="b"><f>TRUE()</f><v>1</v></c>
      <c r="G5" s="1"><f>TODAY()</f><v>45000</v></c><c r="H5" s="2"><f>NOW()</f><v>45000.5</v></c>
      <c r="I5" s="3"><f>0.5</f><v>0.5</v></c><c r="J5" t="e"><f>1/0</f><v>#DIV/0!</v></c>
      <c r="K5" t="str"><f>""</f><v></v></c><c r="L5"><f>1+1</f></c></row>
    <row r="6"><c r="B6" t="str"><v>link</v></c><c r="C6"><v>7</v></c></row>
    <row r="7"><c r="M7" t="str"><v>last</v></c></row>
    <row r="8"><c r="N8" t="str"><v></v></c></row>
  XML

  def setup
    @dir = Dir.mktmpdir("roo-calamine")
    @csv = File.join(@dir, "out.csv")
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def types_book
    sheet = Workbook.sheet(TYPES, extra: %(<hyperlinks><hyperlink ref="B6" r:id="rId1"/></hyperlinks>))
    two = Workbook.sheet(%(<row r="1"><c r="C1" t="str"><v>x</v></c></row><row r="3"><c r="E3"><v>1</v></c></row>))
    Workbook.write(@dir, "types", sheets: { "Types" => sheet, "Two" => two, "Empty" => Workbook.sheet("") },
                                  sheet_rels: { "Types" => HYPERLINK_RELS },
                                  shared_strings: Workbook.shared_strings("<si><r><rPr><b/></rPr><t>bold</t></r></si>"))
  end

  # The String to_csv returns, or what it raises, then the file it wrote.
  def csv_outcome(workbook, *args, **options)
    FileUtils.rm_f(@csv)
    returned = Oracle.outcome { Oracle.silence { workbook.to_csv(*args, **options) } }
    returned << returned[1].encoding if returned.first == :ok && returned[1].is_a?(String)
    [args, options, returned, (File.binread(@csv) if File.exist?(@csv))]
  end

  SEPARATORS = [{}, { separator: ";" }, { separator: "::" }, { separator: nil }, { separator: :| }, { separator: "§" },
                { separator: "\xA7".b }, { separator: "§".encode("ISO-8859-1") },
                { separator: "\t".encode("UTF-16LE") }].freeze

  def describe_csv(workbook)
    calls = workbook.sheets.flat_map do |sheet|
      SEPARATORS.flat_map do |options|
        [csv_outcome(workbook, sheet:, **options), csv_outcome(workbook, @csv, sheet:, **options)]
      end
    end
    calls + [csv_outcome(workbook), csv_outcome(workbook, @csv), csv_outcome(workbook, nil, "\t", "Two"),
             csv_outcome(workbook, @csv, ";", 1), csv_outcome(workbook, sheet: "Missing")]
  end

  def roo_csv(path, **options)
    Roo::Calamine.enabled = false
    Roo::Excelx.new(path, options).to_csv
  ensure
    Roo::Calamine.enabled = true
  end

  def test_every_cell_type_separator_sheet_and_file
    path = types_book

    assert_equal <<~CSV, roo_csv(path)
      ,,,,,,,,,,,,
      ,"say ""hi"", ok","a;b\tc
      d ☺",,12,1.5,2,1000,0.30000000000000004,12345678901234567890,0.25,3,0
      ,,,,,,,,,,,,
      ,2023-03-15,2023-03-15T12:00:00+00:00,06:00:00,1899-12-31T12:00:00+00:00,"true","false",#DIV/0!,2023-03-15T18:00:00+00:00,,"<html><b>bold</b></html>",,
      ,2,2.5,2,"a""b",true,2023-03-15,2023-03-15T12:00:00+00:00,43200,"#DIV/0!",,,
      ,"https://example.com/?q=""x"",y",7,,,,,,,,,,
      ,,,,,,,,,,,,"last"
    CSV
    [{}, { empty_cell: true }, { no_hyperlinks: true }].each do |options|
      assert_same_as_roo("to_csv #{options}") { describe_csv(Roo::Excelx.new(path, options.dup)) }
    end
  end

  def test_roo_writes_the_csv_itself_with_the_reader_off_or_under_another_roo
    path = types_book
    read = -> { Roo::Excelx.new(path).to_csv }
    ToCsvTest.roo_fields = 0
    roo, = Oracle.read(native: false, &read)

    assert_equal 7 * 13, ToCsvTest.roo_fields
    ToCsvTest.roo_fields = ToCsvTest.native_writes = 0
    native, = Oracle.read(native: true, &read)

    assert_equal roo, native
    assert_equal 0, ToCsvTest.roo_fields
    assert_equal 1, ToCsvTest.native_writes
    with_roo_version("4.1.0") do
      ToCsvTest.roo_fields = 0
      native, = Oracle.read(native: true, &read)

      assert_equal roo, native
      assert_equal 7 * 13, ToCsvTest.roo_fields
    end
  end

  def test_values_and_cells_changed_in_place_before_to_csv
    path = types_book
    change = lambda do
      workbook = Roo::Excelx.new(path)
      row = workbook.row(2) # columns B to M
      row[0].replace('row "value"')
      row[1].replace("")
      workbook.row(4)[9].replace("shared")
      workbook.cell(5, 5).replace("built")
      workbook.sheet_for("Types").cell_at([2, 5]).value = 99.5
      describe_csv(workbook)
    end
    roo, = Oracle.read(native: false, &change)
    _, _, (_, csv), = roo.last.first

    assert_equal %(,"row ""value""",,,99.5,1.5,2,1000,0.30000000000000004,12345678901234567890,0.25,3,0\n), csv.lines[1]
    assert_match(/"shared".*\n,2,2.5,2,"built",/, csv)
    assert_same_as_roo("changed in place", &change)
  end

  # 20,000 lines of "1,2", then a formula whose cached value is empty, which
  # roo raises for after writing the "3," before it.
  def test_an_empty_formula_value_raises_after_the_text_before_it
    rows = Array.new(20_000) { |i| %(<row r="#{i + 1}"><c r="A#{i + 1}"><v>1</v></c><c r="B#{i + 1}"><v>2</v></c></row>) }
    rows << %(<row r="20001"><c r="A20001"><v>3</v></c><c r="B20001"><f>1+1</f><v></v></c></row>)
    path = Workbook.write(@dir, "empty-formula", sheets: { "Sheet1" => Workbook.sheet(rows.join) })
    roo, = Oracle.read(native: false) { csv_outcome(Roo::Excelx.new(path), @csv) }
    _, _, (outcome, error, message), written = roo.last

    assert_equal [:error, RuntimeError, "unhandled onecell-class NilClass", "#{"1,2\n" * 20_000}3,"],
                 [outcome, error, message, written.gsub("\r\n", "\n")]
    assert_same_as_roo("empty formula value") { csv_outcome(Roo::Excelx.new(path), @csv) }
  end

  def with_roo_version(version)
    original = Roo::VERSION
    Roo.send(:remove_const, :VERSION)
    Roo.const_set(:VERSION, version)
    yield
  ensure
    Roo.send(:remove_const, :VERSION)
    Roo.const_set(:VERSION, original)
  end

  def order_book
    sheets = {
      "Out of order" => Workbook.sheet(%(<row r="3"><c r="C3"><v>3</v></c><c r="A3"><v>1</v></c>) +
                                       %(<c r="C3" t="str"><v>last C3</v></c></row>) +
                                       %(<row r="1"><c r="B1"><v>2</v></c></row>)),
      "Merged" => Workbook.sheet(%(<row r="1"><c r="A1" t="str"><v>m</v></c><c r="C1"><v>1</v></c></row>) +
                                 %(<row r="3"><c r="B3"><v>2</v></c></row>),
                                 extra: %(<mergeCells count="1"><mergeCell ref="A1:B2"/></mergeCells>))
    }
    Workbook.write(@dir, "order", sheets:)
  end

  def test_cells_out_of_order_or_merged
    path = order_book
    roo, = Oracle.read(native: false) do
      workbook = Roo::Excelx.new(path, expand_merged_ranges: true)
      [workbook.sheet_for("Out of order").cells.keys, workbook.sheet_for("Merged").cells.keys]
    end

    assert_equal [:ok, [[[3, 3], [3, 1], [1, 2]], [[1, 1], [1, 3], [3, 2], [1, 2], [2, 1], [2, 2]]]], roo
    [{}, { expand_merged_ranges: true }].each do |options|
      assert_same_as_roo("to_csv #{options}") { describe_csv(Roo::Excelx.new(path, options.dup)) }
    end
  end

  def test_keys_written_into_the_cells_hash
    path = order_book

    assert_same_as_roo("keys written into the cells hash") do
      workbook = Roo::Excelx.new(path)
      cells = workbook.sheet_for("Merged").cells
      cells[[2, 2]] = nil
      cells[[0, 1]] = cells[[1, 1]]
      cells[[3.0, 1]] = cells[[1, 3]]
      cells[[1, 0]] = cells[[1, 3]]
      describe_csv(workbook)
    end
  end

  def test_a_cell_written_before_the_bounds_after_they_were_read
    path = types_book
    write = lambda do
      workbook = Roo::Excelx.new(path)
      workbook.first_row("Two")
      cells = workbook.sheet_for("Two").cells
      cells[[1, 1]] = cells[[1, 3]]
      describe_csv(workbook)
    end
    roo, = Oracle.read(native: false, &write)

    assert_equal [:ok, ",,\"x\",,\n,,,,\n,,,,1\n"], roo.last.find { |call| call[1] == { sheet: "Two" } }[2].first(2)
    assert_same_as_roo("written before the bounds", &write)
  end

  CHANGES_AFTER_THE_BOUNDS = {
    last_row_deleted: ->(cells) { cells.delete_if { |(row, _), _| row == 7 } },
    above_the_first_row: ->(cells) { cells[[1, 2]] = cells[[2, 5]] },
    past_the_last_row: ->(cells) { cells[[9, 2]] = cells[[2, 5]] },
    past_the_last_row_out_of_order: ->(cells) { cells[[1, 2]] = cells[[9, 2]] = cells[[2, 5]] },
    past_the_last_row_as_a_float: ->(cells) { cells[[9.0, 2]] = cells[[2, 5]] }
  }.freeze

  # The types book with its "Types" bounds read, then its cells changed.
  def changed_after_the_bounds(change)
    workbook = Roo::Excelx.new(types_book)
    workbook.first_row("Types")
    change.call(workbook.sheet_for("Types").cells)
    workbook
  end

  def test_cells_changed_after_the_bounds_were_read
    deleted = CHANGES_AFTER_THE_BOUNDS[:last_row_deleted]
    roo, = Oracle.read(native: false) { changed_after_the_bounds(deleted).to_csv(sheet: "Types").lines.last }

    assert_equal [:ok, "#{"," * 12}\n"], roo
    CHANGES_AFTER_THE_BOUNDS.each do |name, change|
      assert_same_as_roo(name) { describe_csv(changed_after_the_bounds(change)) }
    end
  end

  def test_the_workbook_bounds_set_the_rows_and_columns
    path = types_book
    [[:last_row, 3], [:last_row, 9], [:last_row, 9.0], [:last_column, 0], [:last_column, 4], [:last_column, 15],
     [:last_column, 15.0]].each do |name, value|
      subclass = Class.new(Roo::Excelx) { define_method(name) { |_sheet = nil| value } }

      assert_same_as_roo("#{name} #{value}") { describe_csv(subclass.new(path)) }
    end
    wider = Class.new(Roo::Excelx) do
      def last_row(_sheet = nil) = 9
      def last_column(_sheet = nil) = 15
    end
    assert_same_as_roo("cells past the sheet's own bounds") do
      workbook = wider.new(path)
      workbook.first_row("Types")
      cells = workbook.sheet_for("Types").cells
      cells[[9, 2]] = cells[[2, 15]] = cells[[2, 5]]
      describe_csv(workbook)
    end
  end

  def test_an_output_record_separator
    path = types_book

    assert_same_as_roo("$\\") do
      Oracle.silence { $OUTPUT_RECORD_SEPARATOR = "!" }
      describe_csv(Roo::Excelx.new(path))
    ensure
      Oracle.silence { $OUTPUT_RECORD_SEPARATOR = nil }
    end
  end

  def assert_roo_raises_on_the_set_cell(call)
    args, options, (outcome, error, message), written = call

    # roo writes the file in text mode, so on Windows its lines end in \r\n.
    assert_equal [[@csv], { sheet: "Sheet1" }, :error, NoMethodError, "1,2,\n3,"],
                 [args, options, outcome, error, written.gsub("\r\n", "\n")]
    # Ruby 3.3 quotes the method name as `empty?', later versions as 'empty?'.
    assert_match(/undefined method .empty\?' for an instance of Roo::Excelx::Cell/, message)
  end

  def test_set_before_to_csv
    rows = %(<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row>) +
           %(<row r="2"><c r="A2"><v>3</v></c><c r="C2"><v>4</v></c></row>)
    path = Workbook.write(@dir, "set", sheets: { "Sheet1" => Workbook.sheet(rows) })
    set = lambda do |*cells|
      workbook = Roo::Excelx.new(path)
      workbook.first_row
      Oracle.silence { cells.each { |key| workbook.set(*key, "x") } }
      describe_csv(workbook)
    end
    roo, = Oracle.read(native: false) { set.call([2, 2]) }

    assert_roo_raises_on_the_set_cell(roo.last[1])
    [[[2, 2]], [[1, 1]], [[9, 9]], [[3, 1], [9, 9]]].each do |cells|
      assert_same_as_roo("set #{cells}") { set.call(*cells) }
    end
    assert_same_as_roo("set before the bounds") do
      workbook = Roo::Excelx.new(path)
      Oracle.silence { workbook.set(2, 2, "x") }
      describe_csv(workbook)
    end
  end
end
