# frozen_string_literal: true

require_relative "test_helper"

# A workbook opened from a path leaves its worksheets in the zip: the native
# reader inflates them, and roo's own code reads them from the file it
# extracts the first time it needs it.
class DeferredSheetsTest < Minitest::Test
  include OracleAssertions

  ROWS = %(<row r="1"><c r="A1"><v>1</v></c><c r="B1" t="inlineStr"><is><t>one</t></is></c></row>
           <row r="2"><c r="A2"><v>2.5</v></c><c r="B2" t="str"><v>two</v></c></row>)
  HYPERLINK = %(<hyperlinks><hyperlink ref="B1" r:id="rId1"/></hyperlinks>)
  HYPERLINK_RELS = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>).freeze

  def setup
    @dir = Dir.mktmpdir("roo-calamine")
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def write(name = "book", extra: "", **)
    Workbook.write(@dir, name, sheets: { "Sheet1" => Workbook.sheet(ROWS, extra:), "Sheet2" => Workbook.sheet(ROWS) },
                               **)
  end

  def extracted_sheets(workbook)
    Dir.children(workbook.instance_variable_get(:@tmpdir)).grep(/roo_sheet/).sort
  end

  def test_the_native_reader_reads_worksheets_from_the_zip
    xlsx = Roo::Excelx.new(write)
    xlsx.cell(1, 1)
    xlsx.each_row_streaming(sheet: "Sheet2") { nil }
    xlsx.parse(headers: true)

    assert_empty extracted_sheets(xlsx)
  end

  def test_hyperlinks_and_merged_ranges_are_read_from_the_zip
    path = write(extra: %(<mergeCells><mergeCell ref="A1:A2"/></mergeCells>#{HYPERLINK}),
                 sheet_rels: { "Sheet1" => HYPERLINK_RELS })
    xlsx = Roo::Excelx.new(path, expand_merged_ranges: true)

    assert_equal ["https://example.com/", 1], [xlsx.hyperlink(1, 2), xlsx.cell(2, 1)]
    assert_empty extracted_sheets(xlsx)
  end

  def test_roo_extracts_a_worksheet_when_it_reads_it_itself
    xlsx = Roo::Excelx.new(write)
    xlsx.sheet_for("Sheet2").dimensions

    assert_equal %w[roo_sheet2], extracted_sheets(xlsx)
  end

  def test_a_workbook_reads_the_same_after_its_file_is_deleted
    { "plain" => {}, "hyperlinked" => { extra: HYPERLINK, sheet_rels: { "Sheet1" => HYPERLINK_RELS } } }
      .each do |label, options|
      runs = 0
      describe = lambda do |turn_off: false|
        path = write("#{label}#{runs += 1}", **options)
        xlsx = Roo::Excelx.new(path)
        File.delete(path)
        Roo::Calamine.enabled = false if turn_off
        [Oracle.describe_cells(xlsx), Oracle.describe_stream(xlsx, {}), xlsx.sheet_for("Sheet2").dimensions]
      end

      assert_same_as_roo("#{label}, native", native_only: false, &describe)
      assert_same_as_roo("#{label}, turned off after opening", native_only: false) { describe.call(turn_off: true) }
    end
  end

  def test_a_path_in_binary_encoding
    path = write("café").b

    assert_same_as_roo("binary path") { Oracle.describe_stream(Roo::Excelx.new(path), {}) }
    xlsx = Roo::Excelx.new(path)
    xlsx.cell(1, 1)

    assert_empty extracted_sheets(xlsx)
  end

  def test_a_workbook_reads_the_same_after_its_file_is_overwritten
    runs = 0
    assert_same_as_roo("overwritten", native_only: false) do
      first = shared_strings_book("first#{runs += 1}", %w[alice bob carol])
      xlsx = Roo::Excelx.new(first)
      IO.copy_stream(shared_strings_book("second#{runs}", %w[XXXXX YYY ZZZZZ], rows: 2), first)
      [Oracle.describe_cells(xlsx), Oracle.describe_stream(xlsx, {})]
    end
  end

  # The two files differ only in a worksheet's digits, so its entry sits at
  # the same offset in both.
  def test_reload_after_turning_the_reader_off_reads_the_file_as_it_is_now
    runs = 0
    numbers = ->(name, digits) { Workbook.write(@dir, name, sheets: { "Sheet1" => Workbook.sheet(digits.map.with_index(1) { |d, r| %(<row r="#{r}"><c r="A#{r}"><v>#{d}</v></c></row>) }.join) }) }
    assert_same_as_roo("reloaded", native_only: false) do
      first = numbers.call("first#{runs += 1}", [1, 2, 3])
      xlsx = Roo::Excelx.new(first)
      IO.copy_stream(numbers.call("second#{runs}", [7, 8, 9]), first)
      Roo::Calamine.enabled = false
      xlsx.reload
      xlsx.column(1)
    end
  end

  def test_a_packed_workbook_named_like_the_copy
    packed = File.join(@dir, "roo_calamine.xlsx.zip")
    book = write
    Zip::File.open(packed, create: true) { |zip| zip.add("roo_calamine.xlsx", book) }

    assert_same_as_roo("packed") { Oracle.describe_stream(Roo::Excelx.new(packed, packed: :zip), {}) }
  end

  def shared_strings_book(name, strings, rows: strings.size)
    xml = (1..rows).map { |r| %(<row r="#{r}"><c r="A#{r}" t="s"><v>#{r - 1}</v></c></row>) }.join
    Workbook.write(@dir, name, sheets: { "Sheet1" => Workbook.sheet(xml) },
                               shared_strings: Workbook.shared_strings(*strings.map { "<si><t>#{_1}</t></si>" }))
  end

  def test_open_workbooks_hold_no_file_descriptor
    skip "no /dev/fd" unless File.directory?("/dev/fd")

    path = write
    before = Dir.children("/dev/fd").size
    workbooks = Array.new(20) { Roo::Excelx.new(path).tap { _1.cell(1, 1) } }

    assert_operator Dir.children("/dev/fd").size, :<, before + workbooks.size
  end

  def test_an_empty_file_raises_as_in_roo
    path = File.join(@dir, "empty.xlsx")
    File.write(path, "")

    assert_same_as_roo("empty", native_only: false) { Roo::Excelx.new(path) }
  end

  def test_a_sheet_read_after_its_workbook_is_gone_raises_as_in_roo
    { "closed" => :close.to_proc, "collected" => ->(xlsx) { Roo::Excelx.finalize_tempdirs(xlsx.object_id) } }
      .each do |label, gone|
      assert_same_as_roo(label, native_only: false) do
        xlsx = Roo::Excelx.new(write)
        sheet = xlsx.sheet_for("Sheet1")
        gone.call(xlsx)
        GC.start
        [Oracle.outcome { sheet.cells }, Oracle.outcome { sheet.dimensions }]
      end
    end
  end

  # roo finds no worksheet file for a chart sheet, and raises reading it.
  def test_a_sheet_without_a_worksheet_raises_as_in_roo
    path = write
    Zip::File.open(path) do |zip|
      rels = zip.read("xl/_rels/workbook.xml.rels").sub(%r{worksheet" Target="worksheets/sheet2}, 'chartsheet" Target="chartsheets/sheet1')
      zip.get_output_stream("xl/_rels/workbook.xml.rels") { |io| io.write(rels) }
    end
    read = -> { Roo::Excelx.new(path).cell(1, 1, "Sheet2") }
    roo, = Oracle.read(native: false, &read)

    assert_equal Roo::FileNotFound, roo[1]
    assert_same_as_roo("chart sheet", native_only: false, &read)
  end

  def test_stored_worksheets
    path = write
    Zip::File.open(path) do |zip|
      xml = zip.read("xl/worksheets/sheet1.xml")
      zip.get_output_stream("xl/worksheets/sheet1.xml", compression_method: Zip::Entry::STORED) { |io| io.write(xml) }
    end
    method = Zip::File.open(path) { |zip| zip.find_entry("xl/worksheets/sheet1.xml").compression_method }

    assert_equal Zip::Entry::STORED, method
    assert_same_as_roo("stored") { Oracle.describe_stream(Roo::Excelx.new(path), {}) }
    assert_same_as_roo("stored, lookups", lazy: true) { Oracle.describe_lookups(Roo::Excelx.new(path)) }
  end

  # The central directory says the worksheet is 10 bytes shorter than it
  # inflates to: rubyzip raises when roo extracts it.
  def test_a_worksheet_larger_than_its_size_raises_as_in_roo
    path = write
    understate_size(path, "xl/worksheets/sheet1.xml", 10)

    assert_same_as_roo("too large", native_only: false) do
      xlsx = Roo::Excelx.new(path)
      [Oracle.describe_stream(xlsx, {}), Oracle.describe_cells(xlsx)]
    end
    roo, = Oracle.read(native: false) { Roo::Excelx.new(path) }

    assert_equal Zip::EntrySizeError, roo[1]
  end

  # rubyzip writes the size in the entry's Zip64 extra field.
  def understate_size(path, name, by)
    zip = File.binread(path)
    header = -1
    header = zip.index("PK\x01\x02".b, header + 1) until zip.byteslice(header + 46, name.bytesize) == name
    field = header + 46 + name.bytesize
    field += 4 + zip.byteslice(field + 2, 2).unpack1("v") until zip.byteslice(field, 2).unpack1("v") == 1
    zip[field + 4, 8] = [zip.byteslice(field + 4, 8).unpack1("Q<") - by].pack("Q<")
    File.binwrite(path, zip)
  end
end
