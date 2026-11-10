# frozen_string_literal: true

require_relative "test_helper"

# Hand-built worksheets for the cases where a reader that does not follow
# roo's own rules would return something else. Each test first pins what roo
# itself returns, so the workbook is known to exercise the case, then asserts
# that roo-calamine returns the same thing on every read path.
class EdgeCasesTest < Minitest::Test
  include OracleAssertions

  HYPERLINK_RELS = %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{Workbook::REL}/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>).freeze

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
    edit_first = lambda do
      Roo::Excelx.new(path).each_row_streaming.map do |row|
        value = row.first.value
        value.replace("edited") if row.first.coordinate.row == 1
        value.dup
      end
    end
    roo, = Oracle.read(native: false, &edit_first)
    native, = Oracle.read(native: true, &edit_first)

    assert_equal [:ok, %w[edited bold]], roo
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

  def test_multiple_sheets_and_a_missing_shared_strings_part
    sheets = { "One" => Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c></row>)),
               "Two" => Workbook.sheet(%(<row r="3"><c r="B3" t="s"><v>0</v></c></row>)) }
    path = Workbook.write(@dir, "book", sheets:)

    assert_every_path_same_as_roo(path, native_only: false)
  end

  # Two sheets; One has a repeated header, a column without a header, and
  # rows with gaps.
  def write_header_book
    one = Workbook.sheet(<<~XML)
      <row r="1"><c r="A1" t="inlineStr"><is><t>name</t></is></c><c r="B1" t="inlineStr"><is><t>qty</t></is></c>
        <c r="C1" t="inlineStr"><is><t>name</t></is></c><c r="E1"><v>5</v></c></row>
      <row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2"><v>1</v></c></row>
      <row r="4"><c r="C4"><v>3</v></c><c r="E4" t="inlineStr"><is><t>e</t></is></c></row>
      <row r="5"><c r="A5" t="inlineStr"><is><t>c</t></is></c></row>
    XML
    two = Workbook.sheet(%(<row r="3"><c r="A3" t="inlineStr"><is><t>other</t></is></c><c r="B3"><v>7</v></c></row>))
    Workbook.write(@dir, "book", sheets: { "One" => one, "Two" => two })
  end

  # The rows each(headers: true) yields, calling `change` after each one with
  # the workbook and how many rows it yielded.
  def each_with_change(path, change)
    xlsx = Roo::Excelx.new(path)
    seen = []
    # Roo::Excelx#set warns that the Cell it builds is deprecated.
    Oracle.silence do
      xlsx.each(headers: true) do |row|
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
