# frozen_string_literal: true

require_relative "test_helper"
require "matrix"

class FormattersTest < Minitest::Test
  include OracleAssertions

  def setup
    @dir = Dir.mktmpdir("roo-calamine-formatters")
    rows = <<~XML
      <row r="2"><c r="B2" t="str"><v>text &amp; &lt;tag&gt; "☺"&#13;&#10;</v></c>
        <c r="C2"><v>1.5</v></c><c r="D2" t="b"><v>0</v></c><c r="E2" t="s"><v>0</v></c></row>
      <row r="4"><c r="B4" s="1"><v>45000</v></c><c r="C4" s="2"><v>45000.5</v></c>
        <c r="D4" s="3"><v>0.25</v></c><c r="E4" s="4"><v>0.25</v></c></row>
      <row r="5"><c r="B5"><f>1+1</f><v>2</v></c><c r="C5" t="str"><f>""</f><v></v></c>
        <c r="D5" t="e"><v>#DIV/0!</v></c><c r="E5"/><c r="F5" t="str"><v>link</v></c></row>
    XML
    rels = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>)
    @path = Workbook.write(@dir, "formatters",
                           sheets: { "Types" => Workbook.sheet(rows, extra: %(<hyperlinks><hyperlink ref="F5" r:id="rId1"/></hyperlinks>)),
                                     "Empty" => Workbook.sheet(%(<row r="1"><c r="A1" t="str"><v></v></c></row>)),
                                     "Order" => Workbook.sheet(%(<row r="3"><c r="C3"><v>1</v></c><c r="A3"><v>2</v></c><c r="C3"><v>3</v></c></row><row r="1"><c r="B1"><v>4</v></c></row>)) },
                           sheet_rels: { "Types" => rels },
                           shared_strings: Workbook.shared_strings("<si><t>shared</t></si>"))
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def outputs(book)
    calls = book.sheets.flat_map do |sheet|
      book.default_sheet = sheet
      [Oracle.outcome { book.to_yaml }, Oracle.outcome { book.to_matrix.to_a },
       Oracle.outcome { book.to_yaml({ "source" => "example" }, 0, 0, 6, 7, sheet) },
       Oracle.outcome { book.to_matrix(0, 0, 6, 7, sheet).to_a }]
    end
    calls + [Oracle.outcome { book.to_yaml({}, 1, 1, 1, 1, "Empty") }, book.to_xml, book.default_sheet]
  end

  def test_output_matches_roo_for_types_rectangles_empty_sheets_and_order
    [{}, { empty_cell: true }].each do |options|
      assert_same_as_roo("formatters #{options}") { outputs(Roo::Excelx.new(@path, options.dup)) }
    end
  end

  def test_output_preserves_values_and_cells_changed_in_place
    [false, true].each do |materialize|
      assert_same_as_roo("changed formatters #{materialize}") do
        book = Roo::Excelx.new(@path)
        book.row(2)[0].replace("changed é".encode("ISO-8859-1"))
        book.row(2)[3].replace("shared\x01changed")
        book.sheet_for("Types").cell_at(2, 3).value = 99
        empty = book.cell(1, 1, "Empty")
        book.first_row("Empty")
        empty.replace("now present")
        if materialize
          cells = book.sheet_for("Types").cells
          cells[[3, 3]] = cells.delete([4, 3])
        end
        outputs(book)
      end
    end
  end

  def test_native_formatters_avoid_per_slot_cell_lookups
    %i[to_yaml to_xml to_matrix].each do |formatter|
      lookups = 0
      trace = TracePoint.new(:call) { |event| lookups += 1 if event.method_id == :cell }
      book = Roo::Excelx.new(@path)
      trace.enable { book.public_send(formatter) }

      assert_equal 0, lookups, "#{formatter} still reads each slot through #cell"
    end
  end
end
