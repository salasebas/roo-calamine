# frozen_string_literal: true

require_relative "test_helper"
require "objspace"

class CellConstructionTest < Minitest::Test
  include OracleAssertions

  def setup
    @dir = Dir.mktmpdir("roo-oxide")
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def write_serials(texts)
    cells = texts.each_with_index.map do |text, index|
      %(<c r="#{Roo::Utils.number_to_letter(index + 1)}1" s="2"><v>#{text}</v></c>)
    end
    Workbook.write(@dir, "serials", sheets: { "Sheet1" => Workbook.sheet(%(<row r="1">#{cells.join}</row>)) })
  end

  def test_late_single_cell_reads_do_not_allocate_for_every_preceding_record
    rows = (1..10_000).map do |row|
      %(<row r="#{row}"><c r="A#{row}"><v>#{row}</v></c><c r="B#{row}" t="inlineStr"><is><t>tail</t></is></c></row>)
    end.join
    path = Workbook.write(@dir, "late", sheets: { "Sheet1" => Workbook.sheet(rows) })
    workbook = Roo::Excelx.new(path)
    workbook.last_row
    GC.disable
    before = ObjectSpace.memsize_of_all(Array)
    values = [workbook.cell(10_000, 1), workbook.cell(10_000, 2)]
    allocated = ObjectSpace.memsize_of_all(Array) - before

    assert_equal [10_000, "tail"], values
    assert_operator allocated, :<, 64 * 1024
  ensure
    GC.enable
  end

  def test_temporal_values_are_read_without_building_full_cells
    path = write_serials(%w[45000.5 0.25 45000 -60000000000000.25])
    workbook = Roo::Excelx.new(path)
    GC.disable
    before = ObjectSpace.each_object(Roo::Excelx::Cell::DateTime).count
    values = workbook.row(1)

    assert_equal [DateTime, Integer, Date, Integer], values.map(&:class)
    workbook.to_csv

    assert_equal before, ObjectSpace.each_object(Roo::Excelx::Cell::DateTime).count

    cells = workbook.sheet_for(nil).cells.values

    assert_same values.first, cells.first.value
    assert_equal 4, ObjectSpace.each_object(Roo::Excelx::Cell::DateTime).count - before
    assert_instance_of DateTime, cells.last.instance_variable_get(:@datetime)
  ensure
    GC.enable
  end

  def test_linked_text_values_do_not_build_full_cells_and_keep_their_identity
    rows = %(<row r="1"><c r="A1" t="inlineStr"><is><t>inline</t></is></c><c r="B1" t="str"><v>text</v></c></row>)
    sheet = Workbook.sheet(rows, extra: %(<hyperlinks><hyperlink ref="A1:B1" r:id="rId1"/></hyperlinks>))
    rels = %(<Relationships><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/"/></Relationships>)
    path = Workbook.write(@dir, "links", sheets: { "Sheet1" => sheet }, sheet_rels: { "Sheet1" => rels })
    workbook = Roo::Excelx.new(path)
    GC.disable
    before = ObjectSpace.each_object(Roo::Excelx::Cell::String).count
    values = workbook.row(1)

    assert_equal before, ObjectSpace.each_object(Roo::Excelx::Cell::String).count
    described = values.map { |value| [value.class, value.to_s, value.href] }

    assert_equal [[Roo::Link, "inline", "https://example.com/"], [Roo::Link, "text", "https://example.com/"]],
                 described

    values.first.replace("edited")
    cells = workbook.sheet_for(nil).cells.values

    assert_same values.first, cells.first.value
    assert_equal %w[inline text], cells.map(&:cell_value)
  ensure
    GC.enable
  end

  def test_temporal_values_keep_roos_rounding_identity_and_full_cell_state
    path = write_serials(%w[45000.1234565 45000.9999999 1.0000009 1.0000011 0.0000005 -0.25 -60000000000000.25])
    [false, true].each do |date1904|
      if date1904
        Zip::File.open(path) do |zip|
          xml = zip.read("xl/workbook.xml").sub("<sheets>", %(<workbookPr date1904="1"/><sheets>))
          zip.get_output_stream("xl/workbook.xml") { |io| io.write(xml) }
        end
      end
      assert_same_as_roo("temporal values then full cells, date1904=#{date1904}", lazy: true) do
        workbook = Roo::Excelx.new(path)
        values = workbook.row(1)
        values.grep(DateTime).each { _1.instance_variable_set(:@edited, true) }
        GC.start
        cells = workbook.sheet_for(nil).cells.values
        [values.map { Oracle.describe_value(_1) }, values.zip(cells).map { |value, cell| value.equal?(cell.value) },
         cells.map { Oracle.describe_cell(_1) }, cells.map { _1.value.instance_variables }]
      end
    end
  end

  def test_header_iteration_does_not_build_full_temporal_cells
    path = write_serials(%w[45000.5 45001.5 45002.5])
    GC.disable

    %i[each parse].each do |method|
      roo, = Oracle.read(native: false) { Roo::Excelx.new(path).public_send(method, headers: true) { |row| row } }
      before = ObjectSpace.each_object(Roo::Excelx::Cell::DateTime).count
      native, = Oracle.read(native: true) do
        Roo::Excelx.new(path).public_send(method, headers: true) { |row| row }
      end

      assert_equal roo, native
      assert_equal before, ObjectSpace.each_object(Roo::Excelx::Cell::DateTime).count,
                   "#{method} built full cells to read their values"
    end
  ensure
    GC.enable
  end

  def test_false_hyperlinks_keep_temporal_value_identity
    sheet = Workbook.sheet(%(<row r="1"><c r="A1" s="1"><v>45000</v></c></row>))
    path = Workbook.write(@dir, "false_link", sheets: { "Sheet1" => sheet })
    read = lambda do
      workbook = Roo::Excelx.new(path)
      workbook.sheet_for(nil).hyperlinks[[1, 1]] = false
      value = workbook.row(1).first
      value.equal?(workbook.sheet_for(nil).cells[[1, 1]].value)
    end

    assert_equal [:ok, true], Oracle.read(native: false, &read).first
    assert_same_as_roo("false hyperlink value identity", lazy: true, &read)
  end

  def test_temporal_overflow_keeps_roos_values_and_errors
    %w[-1e20 1e20 -1e308 1e308].each do |text|
      path = write_serials(["45000.5", text])

      assert_same_as_roo("temporal overflow #{text}") do
        workbook = Roo::Excelx.new(path)
        [workbook.row(1), Oracle.describe_cells(workbook)]
      end
      assert_values_same_as_roo("temporal overflow #{text}", path)
    end
  end
end
