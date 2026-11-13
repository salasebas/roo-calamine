# frozen_string_literal: true

require_relative "test_helper"

class CleanTest < Minitest::Test
  include OracleAssertions

  def setup
    @dir = Dir.mktmpdir("roo-oxide-clean")
    rows = <<~XML
      <row r="1"><c r="A1" t="inlineStr"><is><t> name&#9; </t></is></c>
        <c r="B1" t="str"><v> value </v></c><c r="C1" t="str"><v> link </v></c></row>
      <row r="2"><c r="A2" t="s"><v>0</v></c><c r="B2" t="str"><f>"text"</f><v> a&#9;b&#10;c&#13; </v></c>
        <c r="C2" t="s"><v>0</v></c></row>
      <row r="3"><c r="A3" t="s"><v>0</v></c><c r="B3"><v>12</v></c>
        <c r="C3" t="inlineStr"><is><t> raw&#9; link </t></is></c></row>
      <row r="4"><c r="A4" t="inlineStr"><is><t>&#160; &#9;&#10; </t></is></c></row>
    XML
    rels = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>)
    @path = Workbook.write(@dir, "clean", sheets: {
                             "One" => Workbook.sheet(rows, extra: %(<hyperlinks><hyperlink ref="C2" r:id="rId1"/><hyperlink ref="C3" r:id="rId1"/></hyperlinks>)),
                             "Two" => Workbook.sheet(%(<row r="1"><c r="A1" t="s"><v>0</v></c></row>))
                           }, shared_strings: Workbook.shared_strings("<si><t> &#160; shared&#9; text &#160; </t></si>"),
                                          sheet_rels: { "One" => rels })
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def test_clean_matches_roo_before_and_after_materializing_cells
    %i[none cell row cells].product(%i[each parse]).each do |read, iterate|
      assert_same_as_roo("#{read}, #{iterate}, clean") do
        book = Roo::Excelx.new(@path)
        held = values_before_clean(book, read)
        options = { clean: true, headers: true }
        rows = book.public_send(iterate, options).to_a
        first = [Oracle.describe_rows(rows), options, held&.map { |v| Oracle.describe_value(v) },
                 book.first_row, book.last_row, book.to_csv, book.to_yaml, book.to_xml]
        book.default_sheet = "One"
        book.cell(3, 1).replace(" changed ")
        again = Oracle.describe_rows(book.each(clean: true, headers: true).to_a)
        [first, again, Oracle.describe_cells(book), book.sheet("Two").cell(1, 1)]
      end
    end
  end

  def values_before_clean(book, read)
    case read
    when :cell then [book.cell(2, 1), book.cell(2, 2), book.cell(2, 3), book.cell(3, 3)]
    when :row then book.row(2) + book.row(3)
    when :cells then book.sheet_for(nil).cells.values.map(&:value)
    end
  end

  def test_clean_keeps_rows_lazy_and_preserves_raw_cell_values
    book = Roo::Excelx.new(@path)
    rows = book.each(clean: true, headers: true).to_a

    assert_equal [{ "name" => "name", "value" => "value", "link" => "link" },
                  { "name" => "shared text", "value" => "abc", "link" => "shared text" },
                  { "name" => "shared text", "value" => 12, "link" => "raw link" }], rows
    refute_nil book.sheet_for(nil).lazy_cells
    assert_equal " a\tb\nc\r ", book.excelx_value(2, 2)
    assert_equal "abc", book.cell(2, 2)
    refute_instance_of Roo::Link, book.cell(2, 3)
    assert_equal [" raw\t link ", "raw link"], [book.excelx_value(3, 3), book.cell(3, 3)]
  end
end
