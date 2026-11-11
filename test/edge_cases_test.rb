# frozen_string_literal: true

require_relative "test_helper"

# Hand-built worksheets for the cases where a reader that does not follow
# roo's own rules would return something else. Each test first pins what roo
# itself returns, so the workbook is known to exercise the case, then asserts
# that roo-calamine returns the same thing on every read path.
class EdgeCasesTest < Minitest::Test
  include OracleAssertions

  HYPERLINK_RELS = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>).freeze

  TWO_HYPERLINK_RELS = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">) +
                       %(<Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/" TargetMode="External"/>) +
                       %(<Relationship Id="rId2" Type="#{Workbook::REL}/hyperlink" Target="https://example.org/?a=1&amp;b=2" TargetMode="External"/></Relationships>)

  LINKS_AND_MERGES = <<~XML.freeze
    <sheetPr><extLst><hyperlinks><hyperlink ref="A3" r:id="rId1"/></hyperlinks></extLst></sheetPr>
    <x:mergeCells xmlns:x="#{Workbook::MAIN}"><x:mergeCell ref="A1:B2"/><mergeCell ref="B2:C3"/><mergeCell ref="D4:D4"/></x:mergeCells>
    <hyperlinks>
      <hyperlink ref="A1" r:id="rId1"/>
      <hyperlink ref="B1:C2" r:id="rId2" location="s&amp;1&#10;&#x9;&#13;&lt;\t|\r\n|\n|\r|"/>
      <hyperlink ref="A1" r:id="rId2"/>
      <hyperlink ref="D1" location="Sheet1!A1"/>
      <hyperlink ref="D2" r:id="rId9"/>
      <hyperlink ref="D3" id="rId1" r:id="rId2"/>
      <hyperlink r:id="rId1" ref="A4" id="rId2"/>
      <hyperlink r:id="rId1"/>
      <hyperlink ref="B4" r:id="rId1"><hyperlink ref="C4" r:id="rId1"/></hyperlink>
    </hyperlinks>
    <mergeCells><mergeCell ref="C1:D1"/></mergeCells>
    <x:hyperlinks xmlns:x="#{Workbook::MAIN}" xmlns:rel="#{Workbook::REL}"><x:hyperlink ref="C3" rel:id="rId1" location="x"/></x:hyperlinks>
  XML

  def setup
    @dir = Dir.mktmpdir("roo-calamine")
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def write(rows, **)
    @books = (@books || 0) + 1
    Workbook.write(@dir, "book#{@books}", sheets: { "Sheet1" => Workbook.sheet(rows) }, **)
  end

  def assert_every_path_same_as_roo(path, native_only: true, **open_options)
    assert_same_as_roo("cells", native_only:) { Oracle.describe_cells(Roo::Excelx.new(path, open_options.dup)) }
    assert_same_as_roo("api", native_only:) { Oracle.describe_api(Roo::Excelx.new(path, open_options.dup)) }
    lazy = native_only && !open_options[:expand_merged_ranges]
    assert_same_as_roo("lookups", native_only:, lazy:) { Oracle.describe_lookups(Roo::Excelx.new(path, open_options.dup)) }
    assert_same_as_roo("row reads", native_only:, lazy:) do
      Oracle.describe_row_reads(Roo::Excelx.new(path, open_options.dup))
    end
    Oracle::STREAM_OPTIONS.each do |stream_options|
      assert_same_as_roo("each_row_streaming #{stream_options}", native_only:) do
        Oracle.describe_stream(Roo::Excelx.new(path, open_options.dup), stream_options)
      end
    end
    assert_values_same_as_roo("each_row_values", path, open_options, native_only:)
  end

  def roo_cell(path, row, column)
    Roo::Calamine.enabled = false
    Roo::Excelx.new(path).cell(row, column)
  ensure
    Roo::Calamine.enabled = true
  end

  def roo_sheet_hyperlinks(path)
    Roo::Calamine.enabled = false
    Roo::Excelx.new(path).sheet_for(nil).hyperlinks
  ensure
    Roo::Calamine.enabled = true
  end

  def roo_row(path, row)
    Roo::Calamine.enabled = false
    Roo::Excelx.new(path).row(row)
  ensure
    Roo::Calamine.enabled = true
  end

  def test_integer_or_float_follows_the_written_text
    path = write(<<~XML)
      <row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>1.0</v></c><c r="C1"><v>1E3</v></c>
        <c r="D1"><v>12345678901234567890</v></c><c r="E1" s="5"><v>2</v></c><c r="F1" s="4"><v>1</v></c>
        <c r="G1"><v>-0</v></c><c r="H1" s="7"><v>5</v></c><c r="I1" s="8"><v>7</v></c><c r="J1"><v></v></c></row>
    XML

    assert_equal [Integer, Float, Float, Integer, Float],
                 [roo_cell(path, 1, 1), roo_cell(path, 1, 2), roo_cell(path, 1, 3), roo_cell(path, 1, 4),
                  roo_cell(path, 1, 5)].map(&:class)
    assert_every_path_same_as_roo(path)
  end

  def test_dates_datetimes_and_times_follow_the_style_and_the_value
    path = write(<<~XML)
      <row r="1"><c r="A1" s="1"><v>45000</v></c><c r="B1" s="2"><v>45000.5</v></c><c r="C1" s="2"><v>45000</v></c>
        <c r="D1" s="3"><v>0.25</v></c><c r="E1" s="6"><v>45000.75</v></c><c r="F1" s="1"><v>45000.9</v></c>
        <c r="G1" s="3"><v>1.5</v></c><c r="H1" s="1"><v>#N/A</v></c></row>
    XML

    assert_equal [Date, DateTime, Date, Integer],
                 [roo_cell(path, 1, 1), roo_cell(path, 1, 2), roo_cell(path, 1, 3), roo_cell(path, 1, 4)].map(&:class)
    assert_every_path_same_as_roo(path)
  end

  def test_the_1904_date_system
    path = write(%(<row r="1"><c r="A1" s="1"><v>1</v></c></row>))
    Zip::File.open(path) do |zip|
      xml = zip.read("xl/workbook.xml").sub("<sheets>", %(<workbookPr date1904="1"/><sheets>))
      zip.get_output_stream("xl/workbook.xml") { |io| io.write(xml) }
    end

    assert_equal Date.new(1904, 1, 2), roo_cell(path, 1, 1)
    assert_every_path_same_as_roo(path)
  end

  NUMBER_STYLES = [0, 4, 5, 7, 8].freeze
  DATE_STYLES = [1, 2, 3, 6].freeze
  UNUSUAL_TEXTS = ["", " 1", "1 ", "+5", "1_000", "0x1A", "1.", ".5", "1e400", "-1e400", "1e-400", "4.9e-324",
                   "2e-320", "9223372036854775807", "9223372036854775808", "-9223372036854775809", "#N/A", "abc",
                   "TRUE", "12855504354071923631583389444689181878463593399757479065157632.5"].freeze

  def random_digits(random, lengths)
    Array.new(random.rand(lengths)) { random.rand(10) }.join
  end

  def written_numbers(random, count)
    Array.new(count) do
      sign = random.rand(4).zero? ? "-" : ""
      fraction = random.rand(2).zero? ? "" : ".#{random_digits(random, 1..25)}"
      exponent = random.rand(4).zero? ? "#{%w[e E].sample(random:)}#{["", "+", "-"].sample(random:)}#{random.rand(280)}" : ""
      "#{sign}#{random_digits(random, 1..22)}#{fraction}#{exponent}"
    end
  end

  def written_serials(random, count)
    Array.new(count) do
      case random.rand(3)
      when 0 then format("%.#{random.rand(0..17)}f", (random.rand * 200_000) - 1000)
      when 1 then format("%.17g", random.rand(-1000..100_000) + ((random.rand(86_400) + 0.5) / 86_400))
      else format("%.17g", random.rand)
      end
    end
  end

  def cells_xml(texts, styles, row: 1)
    cells = texts.product(styles).each_with_index.map do |(text, style), index|
      type = style == :inline ? %( t="inlineStr") : %( s="#{style}")
      formula = (index % 7).zero? ? "<f>A1</f>" : ""
      %(<c r="#{Roo::Utils.number_to_letter(index + 1)}#{row}"#{type}>#{formula}<v>#{text}</v></c>)
    end
    %(<row r="#{row}">#{cells.join}</row>)
  end

  def booleans_xml(row:)
    cells = %w[0 1 2 01 -1 TRUE].each_with_index.map do |text, index|
      %(<c r="#{Roo::Utils.number_to_letter(index + 1)}#{row}" t="b"><v>#{text}</v></c>)
    end
    %(<row r="#{row}">#{cells.join}</row>)
  end

  def test_written_numbers_dates_and_times
    random = Random.new(4)
    numbers = write(cells_xml(written_numbers(random, 400), NUMBER_STYLES + [:inline]) + booleans_xml(row: 2))
    dates = write(cells_xml(written_serials(random, 300) + %w[0 -0 1 0.5 1.0000001 45000.9999999], DATE_STYLES))

    assert_equal [Float, Integer], roo_row(numbers, 1).map(&:class).uniq.sort_by(&:name)
    assert_equal [Date, DateTime, Integer], roo_row(dates, 1).map(&:class).uniq.sort_by(&:name)
    assert_every_path_same_as_roo(numbers)
    assert_every_path_same_as_roo(dates)
  end

  def test_unusual_number_text_gets_roos_answer
    UNUSUAL_TEXTS.each do |text|
      [NUMBER_STYLES + [:inline], DATE_STYLES].each do |styles|
        path = write(cells_xml([text], styles))

        assert_same_as_roo("#{text.inspect} under styles #{styles}", native_only: false) do
          Oracle.describe_cells(Roo::Excelx.new(path))
        end
      end
    end
  end

  def test_cells_are_built_without_their_constructors_only_under_a_known_roo
    path = write(<<~XML)
      <row r="1"><c r="A1"><v>12</v></c><c r="B1" s="5"><v>1.5</v></c><c r="C1" s="1"><v>45000</v></c>
        <c r="D1" s="2"><v>45000.5</v></c><c r="E1" s="3"><v>0.25</v></c><c r="F1" t="b"><v>1</v></c>
        <c r="G1" t="str"><v>x</v></c><c r="H1" t="inlineStr"><is><t>y</t></is></c><c r="I1" s="4"><f>A1</f><v>0.5</v></c>
        <c r="J1"/></row>
    XML
    read = -> { Oracle.describe_cells(Roo::Excelx.new(path, empty_cell: true)) }
    (_, cells), counts = Oracle.read(native: true, &read)

    assert_equal 10, cells.first[1].size
    assert_equal 0, counts[:constructors]

    with_roo_version("4.1.0") do
      assert_same_as_roo("cells under another roo", &read)
      _, counts = Oracle.read(native: true, &read)

      assert_equal 20, counts[:constructors], "every cell and coordinate should come from its constructor"
      assert_same_as_roo("row reads under another roo") do
        Oracle.describe_row_reads(Roo::Excelx.new(path, empty_cell: true))
      end
    end
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

  def test_booleans_errors_and_typed_strings
    path = write(<<~XML)
      <row r="1"><c r="A1" t="b"><v>1</v></c><c r="B1" t="b"><v>0</v></c><c r="C1" t="e"><v>#DIV/0!</v></c>
        <c r="D1" t="str"><v>a &amp; b &lt;c&gt; &#x263A;</v></c><c r="E1" t="inlineStr"><v>42</v></c>
        <c r="F1" t="str"><v><![CDATA[<raw>]]></v></c><c r="G1" t="n"><v>3</v></c></row>
    XML

    values = (1..4).map { |c| roo_cell(path, 1, c) }

    assert_equal [true, false, "#DIV/0!", "a & b <c> ☺"], values
    assert_every_path_same_as_roo(path)
  end

  def test_shared_strings_including_rich_text
    shared = Workbook.shared_strings(
      "<si><t>plain</t></si>",
      "<si><r><rPr><b/></rPr><t>bold</t></r><r><t> rest</t></r></si>",
      %(<si><r><rPr><vertAlign val="superscript"/></rPr><t>2</t></r></si>),
      "<si><t>line_x000D_break</t></si>",
      "<si><t>&lt;b&gt;looks like html</t></si>",
      %(<si><t xml:space="preserve">  spaced  </t><rPh><t>phonetic</t></rPh></si>)
    )
    cells = (0..5).map { |i| %(<c r="#{("A".ord + i).chr}1" t="s"><v>#{i}</v></c>) }.join
    path = write(%(<row r="1">#{cells}</row><row r="2"><c r="A2" t="s"><v>0</v></c><c r="B2" t="s"><v> 1</v></c></row>),
                 shared_strings: shared)

    assert_equal "<html><b>bold</b> rest</html>", roo_cell(path, 1, 2)
    assert_every_path_same_as_roo(path)
    assert_every_path_same_as_roo(path, disable_html_wrapper: true)
  end

  def test_rich_text_is_chosen_again_for_every_cell
    shared = Workbook.shared_strings("<si><r><rPr><b/></rPr><t>bold</t></r></si>")
    path = write(%(<row r="1"><c r="A1" t="s"><v>0</v></c></row><row r="2"><c r="A2" t="s"><v>0</v></c></row>),
                 shared_strings: shared)
    # roo hands out its own shared-string objects, so changing one in place
    # changes what roo's use_html? sees for the next cell.
    edit = ->(value, index) { (index.zero? ? value.replace("edited") : value).dup }
    edit_first = -> { Roo::Excelx.new(path).each_row_streaming.with_index.map { |row, i| edit.call(row.first.value, i) } }
    edit_first_value = -> { Roo::Calamine.each_row_values(path).with_index.map { |(value), i| edit.call(value, i) } }
    roo, = Oracle.read(native: false, &edit_first)
    native, = Oracle.read(native: true, &edit_first)
    native_values, = Oracle.read(native: true, &edit_first_value)

    assert_equal [:ok, %w[edited bold]], roo
    assert_equal roo, native
    assert_equal roo, native_values
  end

  # roo hands out the format String of its own styles table, so changing it
  # in place changes how roo reads the next cell with that style.
  def stream_editing_the_format(texts = %w[2 2 2])
    styles = %(<styleSheet xmlns="#{Workbook::MAIN}"><numFmts count="1"><numFmt numFmtId="164" formatCode="0"/></numFmts>) +
             %(<cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="164"/></cellXfs></styleSheet>)
    rows = texts.each_with_index.map { |text, i| %(<row r="#{i + 1}"><c r="A#{i + 1}" s="1"><v>#{text}</v></c></row>) }
    path = write(rows.join, styles:)
    stream = lambda do
      Roo::Excelx.new(path).each_row_streaming.map do |row|
        yield row.first
        row.first.value
      end
    end
    [Oracle.read(native: false, &stream).first, Oracle.read(native: true, &stream).first]
  end

  def test_a_format_changed_in_place_is_read_again_for_the_next_cell
    edits = { 1 => "0.00", 2 => "yyyy-mm-dd" }
    roo, native = stream_editing_the_format do |cell|
      cell.format.replace(edits[cell.coordinate.row]) if edits.key?(cell.coordinate.row)
    end

    assert_equal [:ok, [2, 2.0, Date.new(1900, 1, 1)]], roo
    assert_equal roo, native
  end

  def test_a_format_whose_match_raises_is_left_to_roo
    break_format = ->(cell) { cell.format.replace("#\xff").force_encoding(Encoding::US_ASCII) }
    roo, native = stream_editing_the_format(["2", "#N/A", ""], &break_format)

    assert_equal [:ok, [2, "#N/A", nil]], roo
    assert_equal roo, native

    roo, native = stream_editing_the_format(%w[2 2], &break_format)

    assert_equal [:error, ArgumentError], roo.first(2)
    assert_equal roo, native
  end

  def test_a_format_given_another_encoding_is_read_again_for_the_next_cell
    roo, native = stream_editing_the_format { |cell| cell.format.force_encoding(Encoding::UTF_16LE) }

    assert_equal :error, roo.first
    assert_equal roo, native
  end

  def test_formulas_and_their_cached_values
    path = write(<<~XML)
      <row r="1"><c r="A1"><f>1+1</f><v>2</v></c><c r="B1"><f t="shared" ref="B1:B3" si="0">A1*2</f><v>4</v></c></row>
      <row r="2"><c r="B2"><f t="shared" si="0"/><v>0</v></c><c r="C2" t="str"><f>"x"&amp;"y"</f><v>xy</v></c></row>
      <row r="3"><c r="B3"><f t="shared" si="0"/></c><c r="C3" t="e"><f>1/0</f><v>#DIV/0!</v></c><c r="D3"><v>1</v><f>late</f></c></row>
    XML

    assert_equal "", Roo::Excelx.new(path).formula(2, 2)
    assert_every_path_same_as_roo(path)
  end

  def test_coordinates_padding_and_empty_cells
    path = write(<<~XML)
      <row r="2"><c r="C2"><v>1</v></c><c r="F2"/><c r="H2" t="inlineStr"><is><t></t></is></c></row>
      <row><c><v>2</v></c><c><v>3</v></c></row>
      <row r="7"/>
      <row r="9"><c r="aa9"><v>4</v></c></row>
    XML

    assert_every_path_same_as_roo(path, native_only: false)
    assert_every_path_same_as_roo(path, empty_cell: true, native_only: false)
  end

  def test_inline_strings_take_every_t_element
    path = write(<<~XML)
      <row r="1"><c r="A1" t="inlineStr"><is><r><t>a</t></r><r><rPr><b/></rPr><t>b</t></r><rPh><t>c</t></rPh></is></c>
        <c r="B1" t="inlineStr"><is><t>line&#10;two</t></is></c></row>
    XML

    assert_equal "abc", roo_cell(path, 1, 1)
    assert_every_path_same_as_roo(path)
  end

  def test_merged_ranges_and_hyperlinks
    sheet = Workbook.sheet(
      %(<row r="1"><c r="A1"><v>1</v></c><c r="B1" t="str"><v>link</v></c></row><row r="2"><c r="A2"/></row>),
      extra: %(<mergeCells count="1"><mergeCell ref="A1:A2"/></mergeCells><hyperlinks><hyperlink ref="B1" r:id="rId1" location="top"/></hyperlinks>)
    )
    path = Workbook.write(@dir, "book", sheets: { "Sheet1" => sheet }, sheet_rels: { "Sheet1" => HYPERLINK_RELS })

    assert_equal "https://example.com/#top", roo_cell(path, 1, 2).href
    assert_every_path_same_as_roo(path)
    assert_every_path_same_as_roo(path, expand_merged_ranges: true)
    assert_every_path_same_as_roo(path, no_hyperlinks: true)
  end

  def test_links_and_merges_are_read_natively_only_under_a_known_roo
    sheet = Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c><c r="B1" t="str"><v>link</v></c></row>),
                           extra: %(<mergeCells><mergeCell ref="A1:B1"/></mergeCells><hyperlinks><hyperlink ref="B1" r:id="rId1"/></hyperlinks>))
    path = Workbook.write(@dir, "book", sheets: { "Sheet1" => sheet }, sheet_rels: { "Sheet1" => HYPERLINK_RELS })
    read = -> { Oracle.describe_cells(Roo::Excelx.new(path, expand_merged_ranges: true)) }
    _, counts = Oracle.read(native: true, &read)

    assert_equal 0, counts[:roo_documents]

    with_roo_version("4.1.0") do
      assert_same_as_roo("links and merges under another roo", native_only: false, &read)
      _, counts = Oracle.read(native: true, &read)

      assert_operator counts[:roo_documents], :>, 0, "roo should read the whole document"
    end
  end

  def test_hyperlink_and_merged_range_markup
    rows = (1..4).map { |r| %(<row r="#{r}">#{("A".."D").map { |c| %(<c r="#{c}#{r}" t="str"><v>#{c}#{r}</v></c>) }.join}</row>) }
    path = Workbook.write(@dir, "book", sheets: { "Sheet1" => Workbook.sheet(rows.join, extra: LINKS_AND_MERGES) },
                                        sheet_rels: { "Sheet1" => TWO_HYPERLINK_RELS })
    links = roo_sheet_hyperlinks(path)

    assert_equal "https://example.org/?a=1&b=2", links[[1, 1]]
    assert_equal "https://example.org/?a=1&b=2#s&1\n\t\r< | | | |", links[[2, 3]]
    assert_equal [[1, 1], [1, 2], [1, 3], [2, 2], [2, 3], [3, 4], [4, 1], [4, 2], [3, 3]], links.keys
    assert_every_path_same_as_roo(path)
    assert_every_path_same_as_roo(path, expand_merged_ranges: true)
  end

  def test_merged_ranges_roo_raises_on
    sheet = ->(ref) { Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c></row>), extra: %(<mergeCells><mergeCell #{ref}/></mergeCells>)) }
    paths = { "no ref" => "", "one cell" => %(ref="A1"), "bad reference" => %(ref="1A:B2") }.to_h do |name, ref|
      [name, Workbook.write(@dir, name.tr(" ", "_"), sheets: { "Sheet1" => sheet.call(ref) })]
    end

    paths.each do |name, path|
      roo, = Oracle.read(native: false) { Roo::Excelx.new(path, expand_merged_ranges: true).cell(1, 1) }

      assert_equal :error, roo.first, name
      assert_every_path_same_as_roo(path, native_only: false, expand_merged_ranges: true)
    end
  end

  # libxml2 recovers from these into a document without the hyperlinks and
  # merged ranges that follow them.
  def test_hyperlinks_and_merged_ranges_libxml2_recovers_from_go_to_roo
    extra = %(<mergeCells><mergeCell ref="A1:B1"/></mergeCells><hyperlinks><hyperlink ref="A1" r:id="rId1"/></hyperlinks>)
    broken = { "unclosed" => %(<row r="1"><c r="A1"><v>1</v></row>),
               "closed twice" => %(<row r="1"><c r="A1"><v>1</v></c></c></row>),
               "attributes run together" => %(<row r="1"><c r="A1"s="0"><v>1</v></c></row>),
               "a bad name" => %(<row r="1"><1c r="A1"><v>1</v></1c></row>) }
    broken.each do |name, rows|
      path = Workbook.write(@dir, name.tr(" ", "_"), sheets: { "Sheet1" => Workbook.sheet(rows, extra:) },
                                                     sheet_rels: { "Sheet1" => HYPERLINK_RELS })

      assert_empty roo_sheet_hyperlinks(path), name
      assert_every_path_same_as_roo(path, native_only: false)
      assert_every_path_same_as_roo(path, native_only: false, expand_merged_ranges: true)
    end
  end

  def test_prefixed_namespaces
    xml = %(<x:worksheet xmlns:x="#{Workbook::MAIN}"><x:sheetData><x:row r="1"><x:c r="A1" t="inlineStr"><x:is><x:t>hi</x:t></x:is></x:c><x:c r="B1"><x:v>2</x:v></x:c></x:row></x:sheetData></x:worksheet>)
    path = Workbook.write(@dir, "book", sheets: { "Sheet1" => xml })

    assert_equal "hi", roo_cell(path, 1, 1)
    assert_every_path_same_as_roo(path)
  end

  def test_errors_are_roos_errors
    no_styles = write(%(<row r="1"><c r="A1"><v>1</v></c></row>), styles: nil)
    bad_reference = write(%(<row r="1"><c r="1A"><v>1</v></c></row>))
    bad_number = write(%(<row r="1"><c r="A1"><v>2020-01-01</v></c></row>))
    bad_float = write(%(<row r="1"><c r="A1" s="4"><v>1,5€</v></c></row>))
    bad_integer = write(%(<row r="1"><c r="A1"><v>1,5€</v></c></row>))

    [no_styles, bad_reference, bad_number, bad_float, bad_integer].each do |path|
      roo, = Oracle.read(native: false) { Roo::Excelx.new(path).cell(1, 1) }

      assert_equal :error, roo.first
      assert_every_path_same_as_roo(path, native_only: false)
    end
  end

  # roo builds every cell of the sheet at once, so a cell that raises does so
  # for a read of any other cell too.
  def test_a_cell_that_raises_raises_for_every_read
    linked = %(<hyperlinks><hyperlink ref="B1" r:id="rId1"/></hyperlinks>)
    paths = [%(<c r="B1"><v>abc</v></c>), %(<c r="A1"><v>abc</v></c><c r="A1"><v>1</v></c>),
             %(<c r="B1" s="2"><v>1e400</v></c>), %(<c r="B1" s="2"><v>-1e308</v></c>),
             %(<c r="B1" t="s"><v>5</v></c>)].map do |cells|
      write(%(<row r="1"><c r="A1"><v>1</v></c>#{cells}</row>), shared_strings: Workbook.shared_strings("<si><t>a</t></si>"))
    end
    paths << Workbook.write(
      @dir, "linked_time", sheets: { "Sheet1" => Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c><c r="B1" s="2"><v>45000.5</v></c></row>), extra: linked) },
                           sheet_rels: { "Sheet1" => HYPERLINK_RELS }
    )

    paths.each do |path|
      roo, = Oracle.read(native: false) { Roo::Excelx.new(path).cell(1, 1) }

      assert_equal :error, roo.first, path
      assert_every_path_same_as_roo(path, native_only: false)
    end
  end

  def test_a_repeated_coordinate_keeps_the_last_cell_with_a_value
    path = write(%(<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>5</v></c><c r="A1"><v>2</v></c><c r="B1"/></row>))
    strings = write(%(<row r="1"><c r="A1" t="str"><v>a</v></c><c r="B1" t="str"><v>b</v></c>
      <c r="A1" t="str"><v>c</v></c></row><row r="2"><c r="A2" t="str"><v>d</v></c></row>))

    assert_equal [2, 5], [roo_cell(path, 1, 1), roo_cell(path, 1, 2)]
    assert_equal %w[c b], roo_row(strings, 1)
    [path, strings].each do |repeated|
      assert_every_path_same_as_roo(repeated)
      assert_every_path_same_as_roo(repeated, empty_cell: true)
    end
  end

  def test_empty_strings_are_outside_the_bounds
    path = write(<<~XML, shared_strings: Workbook.shared_strings("<si><t></t></si>"))
      <row r="1"><c r="A1" t="str"><v></v></c></row><row r="2"><c r="B2"><v>1</v></c></row>
      <row r="3"><c r="C3" t="s"><v>0</v></c></row>
    XML
    roo, = Oracle.read(native: false) { Roo::Excelx.new(path).then { |w| Oracle::BOUNDS.map { w.public_send(_1) } } }

    assert_equal [:ok, [2, 2, 2, 2]], roo
    assert_every_path_same_as_roo(path)
  end

  def write_strings_to_edit
    shared = Workbook.shared_strings("<si><r><rPr><b/></rPr><t>bold</t></r></si>")
    write(<<~XML, shared_strings: shared)
      <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>0</v></c><c r="C1" t="s"><v> +0</v></c></row>
      <row r="3"><c r="C3"><v>1</v></c></row><row r="4"><c r="D4" t="str"><v>x</v></c></row>
    XML
  end

  def test_values_changed_in_place_after_a_read_stay_changed
    path = write_strings_to_edit
    edit = lambda do
      workbook = Roo::Excelx.new(path)
      # roo hands out one string object for both cells of a shared string.
      workbook.cell(1, 1).replace("edited")
      workbook.cell(4, 4).clear
      read = [workbook.cell(1, 2), workbook.cell(1, 3), Oracle::BOUNDS.map { workbook.public_send(_1) }]
      Roo::Calamine.enabled = false
      read << workbook.sheet_for(nil).cells.transform_values(&:value)
    end
    roo, = Oracle.read(native: false, &edit)
    native, = Oracle.read(native: true, &edit)

    assert_equal [:ok, ["edited", "edited", [1, 3, 1, 3],
                        { [1, 1] => "edited", [1, 2] => "edited", [1, 3] => "edited", [3, 3] => 1, [4, 4] => "" }]],
                 roo
    assert_equal roo, native
  end

  # roo's rows hold the cells' own values too.
  def test_values_changed_in_place_in_rows_stay_changed
    path = write_strings_to_edit
    edit_rows = lambda do
      workbook = Roo::Excelx.new(path)
      workbook.each { |row| row.grep(String).each { _1 << "!" } }
      workbook.parse(headers: true).each { |row| row.values.grep(String).each { _1 << "?" } }
      [workbook.row(4), workbook.cell(1, 2), workbook.sheet_for(nil).cells.transform_values(&:value)]
    end
    roo, = Oracle.read(native: false, &edit_rows)
    native, = Oracle.read(native: true, &edit_rows)

    assert_equal [nil, nil, nil, "x!?"], roo.dig(1, 0)
    assert_equal roo, native
  end

  def test_a_linked_shared_string_keeps_the_text_it_had_before_any_read
    sheet = Workbook.sheet(%(<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>0</v></c></row>),
                           extra: %(<hyperlinks><hyperlink ref="B1" r:id="rId1"/></hyperlinks>))
    path = Workbook.write(@dir, "linked_shared", sheets: { "Sheet1" => sheet }, sheet_rels: { "Sheet1" => HYPERLINK_RELS },
                                                 shared_strings: Workbook.shared_strings("<si><t>original</t></si>"))
    edit = lambda do
      edited = Roo::Excelx.new(path)
      edited.cell(1, 1).replace("edited")
      cleared = Roo::Excelx.new(path)
      cleared.cell(1, 1).clear
      [Oracle.describe_value(edited.cell(1, 2)), Oracle::BOUNDS.map { cleared.public_send(_1) }]
    end

    assert_equal [:ok, [[Roo::Link, false, "original", "https://example.com/"], [1, 1, 2, 2]]], Oracle.read(native: false, &edit).first
    assert_same_as_roo("linked", lazy: true, &edit)
  end

  def test_a_number_format_changed_in_place_keeps_the_types_read_before
    styles = Workbook::STYLES.sub('formatCode="yyyy\\-mm\\-dd hh:mm"', 'formatCode="0"')
    many_styles = styles.sub("</cellXfs>", "#{%(<xf numFmtId="164" fontId="0"/>) * 69_992}</cellXfs>")
    [[6, styles], [70_000, many_styles]].product(["0.00", "yyyy-mm-dd"]) do |(style, xfs), format|
      path = write(%(<row r="1"><c r="A1" s="#{style}"><v>1</v></c><c r="B1" s="#{style}"><v>2</v></c>) +
                   %(<c r="C1" s="#{style}"><v>#N/A</v></c></row>), styles: xfs)
      edit = lambda do
        workbook = Roo::Excelx.new(path)
        workbook.excelx_format(1, 1).replace(format)
        [workbook.cell(1, 2).class, workbook.celltype(1, 2), workbook.excelx_format(1, 2), workbook.cell(1, 3),
         workbook.sheet_for(nil).cells.transform_values { [_1.value.class, _1.value.equal?(_1.cell_value)] }]
      end

      assert_equal [:ok, [Integer, :float, format, "#N/A",
                          { [1, 1] => [Integer, false], [1, 2] => [Integer, false], [1, 3] => [String, true] }]],
                   Oracle.read(native: false, &edit).first, "style #{style}, #{format}"
      assert_same_as_roo("style #{style}, #{format}", lazy: style == 6, &edit)
    end
  end

  def test_hyperlinks_changed_after_a_read_leave_the_cells_roo_built
    sheet = Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c><c r="B1" t="str"><v>x</v></c></row>),
                           extra: %(<hyperlinks><hyperlink ref="A1" r:id="rId1"/></hyperlinks>))
    path = Workbook.write(@dir, "links_changed", sheets: { "Sheet1" => sheet }, sheet_rels: { "Sheet1" => HYPERLINK_RELS })
    edit = lambda do
      added = Roo::Excelx.new(path)
      added.cell(1, 1)
      added.sheet_for(nil).hyperlinks[[1, 2]] = "http://x"
      cleared = Roo::Excelx.new(path)
      cleared.cell(1, 2)
      cleared.sheet_for(nil).hyperlinks.clear
      [Oracle.describe_value(added.cell(1, 2)), Oracle.describe_value(cleared.cell(1, 1))]
    end

    assert_equal [:ok, [[String, false, "x"], [Roo::Link, false, "1", "https://example.com/"]]], Oracle.read(native: false, &edit).first
    assert_same_as_roo("hyperlinks", lazy: true, &edit)
  end

  def number_cells = ObjectSpace.each_object(Roo::Excelx::Cell::Number).count

  def test_single_cell_reads_build_only_the_cells_they_read
    path = write((1..3).map { |r| %(<row r="#{r}">#{(1..3).map { |c| %(<c r="#{"ABC"[c - 1]}#{r}"><v>#{r * c}</v></c>) }.join}</row>) }.join)
    workbook = Roo::Excelx.new(path)
    GC.disable
    before = number_cells
    built = -> { number_cells - before }
    reads = [workbook.last_row, workbook.last_column, workbook.cell(2, 2), workbook.celltype(2, 2), workbook.empty?(3, 1)]

    assert_equal [3, 3, 4, :float, false], reads
    assert_equal 2, built.call
    assert_equal 9, workbook.sheet_for(nil).cells.size
    assert_equal 9, built.call
  ensure
    GC.enable
  end

  def test_set_after_single_cell_reads
    path = write(%(<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row>))

    assert_same_as_roo("set") do
      workbook = Roo::Excelx.new(path)
      before = [workbook.cell(1, 1), workbook.last_column]
      workbook.set(1, 1, "x")
      workbook.set(3, 3, 9)
      [before, workbook.cell(1, 1), workbook.celltype(1, 1), workbook.cell(3, 3), workbook.cell(1, 2),
       Oracle.describe_cells(workbook)]
    end
  end

  def test_multiple_sheets_and_a_missing_shared_strings_part
    sheets = { "One" => Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c></row>)),
               "Two" => Workbook.sheet(%(<row r="3"><c r="B3" t="s"><v>0</v></c></row>)) }
    path = Workbook.write(@dir, "book", sheets:)

    assert_every_path_same_as_roo(path, native_only: false)
  end

  # Two sheets; One has a repeated header, a column without a header, and
  # rows with gaps. `last_cell` adds a cell to the last row of One.
  def write_header_book(name = "book", last_cell: "")
    one = Workbook.sheet(<<~XML)
      <row r="1"><c r="A1" t="inlineStr"><is><t>name</t></is></c><c r="B1" t="inlineStr"><is><t>qty</t></is></c>
        <c r="C1" t="inlineStr"><is><t>name</t></is></c><c r="E1"><v>5</v></c></row>
      <row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2"><v>1</v></c></row>
      <row r="4"><c r="C4"><v>3</v></c><c r="E4" t="inlineStr"><is><t>e</t></is></c></row>
      <row r="5"><c r="A5" t="inlineStr"><is><t>c</t></is></c>#{last_cell}</row>
    XML
    two = Workbook.sheet(%(<row r="3"><c r="A3" t="inlineStr"><is><t>other</t></is></c><c r="B3"><v>7</v></c></row>))
    Workbook.write(@dir, name, sheets: { "One" => one, "Two" => two })
  end

  # The rows each(options) yields, calling `change` after each one with the
  # workbook and how many rows it yielded.
  def each_with_change(path, change, options = { headers: true }, open_options = {})
    xlsx = Roo::Excelx.new(path, open_options)
    seen = []
    # Roo::Excelx#set warns that the Cell it builds is deprecated.
    Oracle.silence do
      xlsx.each(options) do |row|
        seen << row
        change.call(xlsx, seen.size)
      end
    end
    seen
  end

  def test_each_follows_changes_made_by_its_block
    path = write_header_book
    scenarios = {
      set_a_later_row: ->(xlsx, n) { xlsx.set(4, 2, 99) if n == 1 },
      switch_sheets: ->(xlsx, n) { xlsx.default_sheet = "Two" if n == 2 },
      reload: ->(xlsx, n) { xlsx.reload if n == 2 },
      set_then_reload: ->(xlsx, n) { n == 1 ? xlsx.set(4, 1, "x") : (xlsx.reload if n == 2) }
    }
    roo, = Oracle.read(native: false) { each_with_change(path, scenarios[:set_a_later_row]) }

    assert_equal 99, roo.dig(1, 3, "qty")
    scenarios.each do |name, change|
      assert_same_as_roo(name) { Oracle.describe_rows(each_with_change(path, change)) }
    end
  end

  # Without options, #each reads the lazy cells, or on a sheet built whole
  # (here for expand_merged_ranges) the tests' row index from the second row
  # on.
  ROW_PATHS = [{}, { expand_merged_ranges: true }].freeze

  def test_each_without_options_follows_changes_made_by_its_block
    path = write_header_book
    sparse = write_header_book("sparse", last_cell: %(<c r="Z5"><v>26</v></c>))
    scenarios = {
      set_before_the_index: ->(xlsx, n) { xlsx.set(4, 2, 99) if n == 1 },
      set_after_the_index: ->(xlsx, n) { xlsx.set(4, 2, 99) if n == 2 },
      switch_sheets: ->(xlsx, n) { xlsx.default_sheet = "Two" if n == 2 },
      reload: ->(xlsx, n) { xlsx.reload if n == 2 }
    }
    roo, = Oracle.read(native: false) { each_with_change(path, scenarios[:set_after_the_index], {}) }

    assert_equal 99, roo.dig(1, 3, 1)
    [path, sparse].product(ROW_PATHS, scenarios.to_a).each do |book, open_options, (name, change)|
      assert_same_as_roo("#{name} #{open_options} #{File.basename(book)}") do
        Oracle.describe_rows(each_with_change(book, change, {}, open_options))
      end
    end
  end

  def test_each_without_options_on_blank_sparse_and_empty_sheets
    sheets = {
      # An empty string is no value for the bounds, but #each still yields it.
      "Blank first" => Workbook.sheet(%(<row r="1"><c r="A1" t="str"><v></v></c></row><row r="2"><c r="A2"><v>1</v></c></row>)),
      "Sparse" => Workbook.sheet((1..50).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>#{i}</v></c></row>) }.join +
                                 %(<row r="51"><c r="XFD51"><v>0</v></c></row>)),
      "Empty" => Workbook.sheet("")
    }
    path = Workbook.write(@dir, "book", sheets:)

    assert_every_path_same_as_roo(path)
    # Switched before the index is built, so it would be built for a sheet
    # without bounds.
    assert_same_as_roo("switch to an empty sheet") do
      Oracle.describe_rows(each_with_change(path, ->(xlsx, n) { xlsx.default_sheet = "Empty" if n == 1 }, {}))
    end
  end

  def test_each_without_options_reads_rows_from_the_index
    rows = (1..5).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>#{i}</v></c><c r="B#{i}"><v>#{i}</v></c></row>) }.join
    dense = write(rows)
    sparse = write(rows + %(<row r="6"><c r="XFD6"><v>6</v></c></row>))

    short = write(%(<row r="1"><c r="A1"><v>1</v></c></row><row r="2"><c r="A2"><v>2</v></c></row>))

    # On a sheet built whole (here for expand_merged_ranges), roo's
    # Sheet#row answers the first row, and every row of a sheet too short to
    # repay the index.
    { dense => 1, short => 2, sparse => 1 }.each do |path, rows_from_roo|
      _, counts = Oracle.read(native: true) { Roo::Excelx.new(path, expand_merged_ranges: true).each.to_a }

      assert_equal rows_from_roo, counts[:sheet_rows]
    end
  end

  def test_each_without_options_indexes_cells_not_rows
    path = write((1..3000).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>#{i}</v></c></row>) }.join +
                 %(<row r="100000000"><c r="B100000000"><v>1</v></c></row>))
    script = <<~RUBY
      require ARGV[0]
      require "objspace"
      Roo::Excelx.new(ARGV[1], expand_merged_ranges: true).each_with_index do |_, i|
        next if i <= Roo::Calamine::RowIndex::ROO_ROWS

        print ObjectSpace.memsize_of_all / 1024
        break
      end
    RUBY
    kilobytes = IO.popen([RbConfig.ruby, "-I", File.expand_path("../lib", __dir__), "-e", script, "roo/calamine", path],
                         &:read)

    assert_operator Integer(kilobytes), :<, 300_000
  end

  # A blank String is no value for the bounds, so each sheet's blank cell lies
  # outside them, where the index must not place it.
  OUTSIDE_THE_BOUNDS = {
    "above the first row" => %(<row r="1"><c r="B1" t="str"><v></v></c></row>
      <row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>2</v></c></row>
      <row r="3"><c r="A3"><v>3</v></c></row><row r="4"><c r="A4"><v>4</v></c></row>),
    "right of the last column" => %(<row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>2</v></c><c r="C2" t="str"><v></v></c></row>
      <row r="3"><c r="B3"><v>3</v></c></row><row r="4"><c r="A4"><v>4</v></c><c r="B4"><v>4</v></c></row>),
    "left of the first column" => %(<row r="2"><c r="B2"><v>2</v></c><c r="C2"><v>2</v></c></row>
      <row r="3"><c r="A3" t="str"><v></v></c><c r="B3"><v>3</v></c><c r="C3"><v>3</v></c></row>
      <row r="4"><c r="B4"><v>4</v></c><c r="C4"><v>4</v></c></row>)
  }.freeze

  def test_each_without_options_leaves_out_cells_outside_the_bounds
    OUTSIDE_THE_BOUNDS.each do |name, rows|
      path = write(rows)
      _, counts = Oracle.read(native: true) { Roo::Excelx.new(path, expand_merged_ranges: true).each.to_a }

      assert_equal 1, counts[:sheet_rows], "#{name}: the index did not answer"
      ROW_PATHS.each do |options|
        assert_same_as_roo("#{name} #{options}") { Oracle.describe_rows(Roo::Excelx.new(path, options).each.to_a) }
      end
    end
  end

  def test_each_without_options_uses_a_subclass_row
    path = write((1..5).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>#{i}</v></c></row>) }.join)
    subclass = Class.new(Roo::Excelx) { def row(number, sheet = nil) = super.map { |v| "<#{v}>" } }

    assert_same_as_roo("subclass row") { subclass.new(path).each.to_a }
  end

  def test_each_without_headers_reads_no_cell
    path = write_header_book

    # roo reads no cell for a row without headers, so the block may even
    # close the workbook.
    assert_same_as_roo("close, no headers") do
      xlsx = Roo::Excelx.new(path)
      Oracle.describe_rows(xlsx.each(clean: true).map { |row| xlsx.close || row })
    end
  end

  # Roo::Calamine.enabled = false still runs roo-calamine's Roo::Excelx#each
  # and #parse, so this compares with roo in a process without roo-calamine:
  # roo checks the options and resets the headers before it reads a cell.
  def test_each_and_parse_fail_as_plain_roo_on_a_sheet_that_raises
    path = write(%(<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>abc</v></c></row>))
    script = <<~RUBY
      require ARGV[0]
      calls = [->(x) { x.each(5) {} }, ->(x) { x.each([1]) {} }, ->(x) { x.each({ clean: true }.freeze) {} },
               ->(x) { x.parse("x") }, ->(x) { x.each(headers: true) {} }, ->(x) { x.parse(headers: true) },
               ->(x) { x.parse(o = { clean: true, headers: true }) rescue (raise $!, o.inspect) }]
      p(calls.map do |call|
        x = Roo::Excelx.new(ARGV[1])
        call.call(x)
      rescue StandardError => e
        [e.class, e.message.lines.first.to_s[0, 60], x.headers]
      end)
    RUBY
    roo, calamine = %w[roo roo/calamine].map do |library|
      IO.popen([RbConfig.ruby, "-I", File.expand_path("../lib", __dir__), "-e", script, library, path], &:read)
    end

    assert_match(/NoMethodError.*TypeError.*FrozenError.*TypeError.*ArgumentError.*\[\]\].*\[\]\].*headers\W+true\}/, roo)
    assert_equal roo, calamine
  end

  def test_parse_reads_header_columns_from_the_second_on
    path = write(<<~XML)
      <row r="1"><c r="B1" t="str"><v>h1</v></c><c r="C1" t="str"><v>h2</v></c><c r="D1" t="str"><v>h3</v></c></row>
      <row r="2"><c r="B2"><v>1</v></c><c r="C2"><v>2</v></c><c r="D2"><v>3</v></c></row>
    XML
    roo, = Oracle.read(native: false) { Roo::Excelx.new(path).parse(b: "h2", c: "h3") }

    assert_equal [:ok, [{ b: 2, c: 3 }]], roo
    assert_every_path_same_as_roo(path)
  end

  def test_parse_reads_cells_set_before_it
    path = write_header_book

    assert_same_as_roo("set, then parse") do
      xlsx = Roo::Excelx.new(path)
      # roo computes the bounds only over the cells it read, not over the
      # Cell objects #set builds.
      xlsx.first_row
      Oracle.silence do
        xlsx.set(2, 5, "late")
        xlsx.set(1, 4, "new header")
      end
      xlsx.header_line = 2
      Oracle.describe_rows(xlsx.parse(headers: true) + xlsx.parse(name: "name") + xlsx.sheet("Two").parse(headers: true))
    end
  end

  def test_streaming_hands_over_to_roo_where_the_native_reader_stops
    rows = (1..300).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>#{i}</v></c></row>) }
    rows[250] = %(<row r="251"><c r="A251" t="str"><v>a&#1;b</v></c></row>)
    path = write(rows.join)

    assert_every_path_same_as_roo(path, native_only: false)

    stream = lambda do
      yielded = []
      Roo::Excelx.new(path).each_row_streaming { |row| yielded << row.map(&:value) }
      [yielded.size, nil]
    rescue StandardError => e
      [yielded.size, e.class]
    end
    roo, = Oracle.read(native: false, &stream)
    native, counts = Oracle.read(native: true, &stream)

    # roo raises a little before row 251, where libxml2's read-ahead hits the
    # bad character; roo-calamine yields every row before it, then roo raises.
    assert_equal Nokogiri::XML::SyntaxError, roo.dig(1, 1)
    assert_operator roo.dig(1, 0), :<=, 250
    assert_equal [:ok, [250, Nokogiri::XML::SyntaxError]], native
    assert_equal 1, counts[:native_readers]
    assert_equal 0, counts[:roo_cells], "roo should not build the 250 rows already yielded again"
  end

  def test_roo_reads_on_where_the_native_reader_stops
    rows = (1..300).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>#{i}</v></c></row>) }
    rows[250] = %(<row r="251"><c r="A251" t="inlineStr"><is><x:t xmlns:x="urn:other">a</x:t><t>251</t></is></c></row>)
    path = write(rows.join)
    values = -> { Roo::Calamine.each_row_values(path).map(&:first) }
    roo, = Oracle.read(native: false, &values)
    native, counts = Oracle.read(native: true, &values)

    assert_equal [:ok, [*1..250, "251", *252..300]], roo
    assert_equal roo, native
    assert_equal 50, counts[:roo_cells], "roo should read on from row 251"
    assert_every_path_same_as_roo(path, native_only: false)
  end

  def test_an_exception_while_waiting_for_a_batch_arrives_as_itself
    skip "needs File.mkfifo" unless File.respond_to?(:mkfifo)

    doc = Roo::Excelx.new(write("")).sheet_for(nil).instance_variable_get(:@sheet)
    fifo = File.join(@dir, "sheet.fifo")
    File.mkfifo(fifo)
    doc.instance_variable_set(:@path, fifo)
    # Held open for writing, without writing, the FIFO keeps the reader
    # waiting for a batch. Opening it read-write does not block.
    stop = Class.new(StandardError)
    reader = nil
    stopped = File.open(fifo, File::RDWR) do
      reader = Thread.new do
        doc.each_row_streaming { nil }
      rescue Exception => e # rubocop:disable Lint/RescueException
        e
      end
      sleep 0.3
      reader.raise(stop, "stop requested")
      reader.join(5)
    end

    assert stopped, "the reader did not stop"
    assert_instance_of stop, reader.value
  end

  def test_xml_libxml2_rejects_goes_to_roo
    illegal_reference = write(%(<row r="1"><c r="A1" t="inlineStr"><is><t>a&#1;b</t></is></c><c r="B1"><v>2</v></c></row>))
    illegal_byte = write(%(<row r="1"><c r="A1" t="str"><v>a\u0001b</v></c></row>))
    truncated = Workbook.write(@dir, "truncated",
                               sheets: { "Sheet1" => Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c></row>))[0...-30] })

    undeclared_prefix = write(%(<row r="1"><c r="A1"><v>1</v></c></row><x:row r="2"><c r="A2"><v>2</v></c></x:row>))
    prefixed_type = Workbook.write(@dir, "prefixed",
                                   sheets: { "Sheet1" => Workbook.sheet(%(<row r="1"><c r="A1" r:t="str"><v>1</v></c></row>)) })
    comment_in_row = write(%(<row r="1"><!-- c --><c r="A1"><v>1</v></c></row>))
    pi_in_value = write(%(<row r="1"><c r="A1" t="str"><v>1<? x?></v></c></row>))
    nested_row = write(%(<row r="1"><c r="A1"><extLst><row r="2"><c r="B2"><v>5</v></c></row></extLst></c></row>))
    duplicated_attribute = write(%(<row r="1" r="2"><c r="A1"><v>1</v></c></row>))
    preserved_space = write(%(<row r="1" xml:space="preserve"> <c r="A1"><v>1</v></c> </row>))
    unknown_entity = Workbook.write(@dir, "entity",
                                    sheets: { "Sheet1" => Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c></row>),
                                                                         extra: "<ext>&bogus;</ext>") })

    [illegal_reference, illegal_byte, truncated, undeclared_prefix, prefixed_type, comment_in_row,
     pi_in_value, nested_row, duplicated_attribute, preserved_space, unknown_entity].each do |path|
      assert_every_path_same_as_roo(path, native_only: false)
    end
  end

  def test_kill_switch_restores_roo
    path = write(%(<row r="1"><c r="A1"><v>1</v></c></row>))

    Roo::Calamine.enabled = false
    Oracle.counts.clear
    Roo::Excelx.new(path).cell(1, 1)

    assert_equal 0, Oracle.counts[:native_readers]
    assert_equal 1, Oracle.counts[:roo_cells]
  ensure
    Roo::Calamine.enabled = true
  end

  def test_environment_variable_disables_the_native_reader
    path = write(%(<row r="1"><c r="A1"><v>1</v></c></row>))
    %w[0 false OFF].each do |value|
      ENV["ROO_CALAMINE"] = value
      Oracle.counts.clear
      Roo::Excelx.new(path).each_row_streaming.to_a

      refute_predicate Roo::Calamine, :enabled?
      assert_equal 0, Oracle.counts[:native_readers]
    end
  ensure
    ENV.delete("ROO_CALAMINE")
  end

  def test_enabled_on_cruby_by_default
    assert_predicate Roo::Calamine, :available?
    assert_predicate Roo::Calamine, :enabled?
  end
end
