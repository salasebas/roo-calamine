# frozen_string_literal: true

require_relative "test_helper"

# Hand-built sharedStrings.xml parts for the rules roo 4.0.0's
# Roo::Excelx::SharedStrings follows. Each test pins what roo itself returns,
# then asserts that roo-calamine returns the same arrays, and whether its
# native extension or roo built them.
class SharedStringsTest < Minitest::Test
  include OracleAssertions

  def setup
    @dir = Dir.mktmpdir("roo-calamine")
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def write(*items, xml: Workbook.shared_strings(*items))
    @books = (@books || 0) + 1
    rows = (0...items.size).map { |i| %(<row r="#{i + 1}"><c r="A#{i + 1}" t="s"><v>#{i}</v></c></row>) }.join
    Workbook.write(@dir, "book#{@books}", sheets: { "Sheet1" => Workbook.sheet(rows) }, shared_strings: xml)
  end

  def roo_strings(path, method, **options)
    Roo::Calamine.enabled = false
    workbook = Roo::Excelx.new(path, options)
    workbook.shared_strings.public_send(method)
  ensure
    Roo::Calamine.enabled = true
  end

  def test_x000d_becomes_a_line_feed_in_plain_text_only
    path = write("<si><t>a_x000D_b</t></si>", "<si><r><rPr><b/></rPr><t>bold_x000D_line</t></r></si>",
                 "<si><t>_x000D_x000D_</t></si>", "<si><t>_x000d_</t></si>")

    assert_equal %W[a\nb bold\nline \nx000D_ _x000d_], roo_strings(path, :to_a)
    assert_equal "<html><b>bold_x000D_line</b></html>", roo_strings(path, :to_html)[1]
    assert_shared_strings_same_as_roo("_x000D_", path, native: true)
  end

  def test_x000d_is_replaced_within_each_text_and_cdata_node
    path = write("<si><t>_x00<![CDATA[0D_]]></t></si>", "<si><t><![CDATA[_x000D_]]></t></si>",
                 "<si><t>_x000&#68;_</t></si>", "<si><t><![CDATA[_x00]]><![CDATA[0D_]]></t></si>",
                 "<si><t><![CDATA[_x00]]>0D_</t></si>", "<si><t>&#95;x000D_<![CDATA[a]]>_x000D_</t></si>")

    assert_equal %W[_x000D_ \n \n \n _x000D_ \na\n], roo_strings(path, :to_a)
    assert_shared_strings_same_as_roo("_x000D_ across nodes", path, native: true)
  end

  def test_a_t_of_the_si_replaces_the_plain_text_and_adds_to_the_html
    path = write("<si><r><t>run</t></r><t>plain</t><r><t>more</t></r></si>",
                 "<si><t>first</t><t>second</t></si>", "<si><t>gone</t><t/></si>")

    assert_equal %w[plainmore second] << "", roo_strings(path, :to_a)
    assert_equal ["<html>runplainmore</html>", "<html>firstsecond</html>", "<html>gone</html>"],
                 roo_strings(path, :to_html)
    assert_shared_strings_same_as_roo("si-level t", path, native: true)
  end

  def test_html_tags_follow_roos_order_and_rules
    path = write(
      %(<si><r><rPr><u/><i/><b/><vertAlign val="subscript"/></rPr><t>x</t></r></si>),
      %(<si><r><rPr><vertAlign val="subscript"/></rPr><rPr><vertAlign val="superscript"/></rPr><t>up</t></r></si>),
      %(<si><r><t>before</t><rPr><b/></rPr><t>after</t></r><r><t>next run</t></r></si>),
      %(<si><r><rPr><b val="0"/><i>text</i><vertAlign val="baseline"/></rPr><t/></r></si>),
      %(<si><r><rPr><vertAlign val="&#115;ubscript"/><sz val="9"/></rPr><t>&lt;b&gt;</t></r></si>),
      %(<si><r><rPr><extLst><b/></extLst></rPr><t>deep</t></r></si>)
    )

    assert_equal ["<html><sub><b><i><u>x</u></i></b></sub></html>", "<html><sup>up</sup></html>",
                  "<html>before<b>after</b>next run</html>", "<html><b><i></i></b></html>",
                  "<html><sub><b></sub></html>", "<html>deep</html>"],
                 roo_strings(path, :to_html)
    assert_shared_strings_same_as_roo("tags", path, native: true)
    assert_shared_strings_same_as_roo("tags without html", path, native: true, disable_html_wrapper: true)
  end

  def test_a_vert_align_without_val_is_left_to_roo
    path = write(%(<si><r><rPr><vertAlign/></rPr><t>x</t></r></si>))

    assert_equal ["x"], roo_strings(path, :to_a)
    assert_raises(NoMethodError) { roo_strings(path, :to_html) }
    assert_shared_strings_same_as_roo("vertAlign", path, native: false)
  end

  def test_elements_match_by_local_name_only_under_sst_si
    prefixed = write(xml: %(<x:sst xmlns:x="#{Workbook::MAIN}"><x:si><x:r><x:rPr><x:b/></x:rPr><x:t>p</x:t></x:r></x:si></x:sst>))
    other = write(
      %(<si><o:t xmlns:o="urn:other">other namespace</o:t></si>),
      "<si>loose text<foo><si><t>nested si</t></si><r><t>nested r</t></r></foo><r>run text<t>x</t></r></si>",
      "<si><r><rPr><t>in rPr</t></rPr><foo><t>in foo</t></foo></r></si>",
      xml: %(<sst xmlns="#{Workbook::MAIN}"><si><o:t xmlns:o="urn:other">other namespace</o:t></si>) +
           "<si>loose text<foo><si><t>nested si</t></si><r><t>nested r</t></r></foo><r>run text<t>x</t></r></si>" \
           "<si><r><rPr><t>in rPr</t></rPr><foo><t>in foo</t></foo></r></si><foo><si><t>outside</t></si></foo></sst>"
    )
    not_sst = write(xml: %(<other xmlns="#{Workbook::MAIN}"><si><t>x</t></si></other>))

    assert_equal ["<html><b>p</b></html>"], roo_strings(prefixed, :to_html)
    assert_equal ["other namespace", "x", ""], roo_strings(other, :to_a)
    assert_empty roo_strings(not_sst, :to_a)
    [prefixed, other, not_sst].each { |path| assert_shared_strings_same_as_roo(path, path, native: true) }
  end

  def test_empty_items_and_parts
    items = write("<si/>", "<si><t/></si>", "<si><r/></si>", "<si><r><rPr/><t></t></r></si>")
    empty = write(xml: %(<sst xmlns="#{Workbook::MAIN}"/>))

    assert_equal ["<html></html>"] * 4, roo_strings(items, :to_html)
    assert_shared_strings_same_as_roo("empty items", items, native: true)
    assert_shared_strings_same_as_roo("empty part", empty, native: true)
  end

  def test_line_endings_and_references
    path = write("<si><t>a\r\nb\rc</t></si>", "<si><t>a&#13;b&#xD;&#10;&amp;&apos;&quot;&lt;&gt;</t></si>",
                 "<si><t><![CDATA[x\r\ny]]></t></si>", "<si><t xml:space=\"preserve\">  é  </t></si>")

    assert_equal ["a\nb\nc", "a\rb\r\n&'\"<>", "x\ny", "  é  "], roo_strings(path, :to_a)
    assert_shared_strings_same_as_roo("line endings", path, native: true)
  end

  def test_strings_are_unfrozen_utf8_objects_of_their_own
    path = write("<si><t>same</t></si>", "<si><t>same</t></si>", "<si><t/></si>", "<si><t/></si>")
    strings = Roo::Excelx.new(path).shared_strings
    all = strings.to_a + strings.to_html

    assert_equal all.size, all.map(&:object_id).uniq.size
    assert(all.none?(&:frozen?))
    assert(all.all? { _1.encoding == Encoding::UTF_8 })
    assert_shared_strings_same_as_roo("objects", path, native: true)
  end

  def test_the_html_strings_are_built_only_when_they_can_be_used
    path = write("<si><r><rPr><b/></rPr><t>bold</t></r></si>")
    workbook = Roo::Excelx.new(path, disable_html_wrapper: true)
    strings = workbook.shared_strings

    assert_equal ["bold"], strings.to_a
    assert_nil strings.instance_variable_get(:@html)
    assert_equal ["<html><b>bold</b></html>"], strings.to_html
  end

  def test_xml_nokogiri_reads_differently_goes_to_roo
    main = %(xmlns="#{Workbook::MAIN}")
    parts = [
      %(<!DOCTYPE sst [<!ENTITY e "entity">]><sst #{main}><si><t>&e;</t></si></sst>),
      %(<?xml version="1.0" encoding="ISO-8859-1"?><sst #{main}><si><t>caf\xE9</t></si></sst>).b,
      %(<sst #{main}><si><t>&bogus;</t></si></sst>),
      %(<sst #{main}><si><t>truncated</t></si><si><t>a),
      %(<sst #{main}><si><t>a&#1;b</t></si></sst>),
      %(<sst #{main}><si><x:t>undeclared</x:t></si></sst>),
      %(<sst #{main}><si><t>a<b>element</b></t></si></sst>),
      %(<sst #{main}><si><t>a<!-- comment -->b</t></si></sst>),
      %(<sst #{main}><si><t><!-- only a comment --></t></si><si><t>_x000D_</t></si></sst>),
      %(<sst #{main}><si><t>a<?pi x?>b</t></si></sst>),
      %(<sst #{main}><si><?t named like a t?></si></sst>),
      %(<sst #{main}><si><r><rPr><?b?></rPr><t>x</t></r></si></sst>),
      %(<sst #{main}><si><t>a</t><x_x000D_y/></si></sst>)
    ]
    paths = parts.map { |xml| write("<si/>", xml:) }

    parts.zip(paths).each { |xml, path| assert_shared_strings_same_as_roo(xml, path, native: false) }
  end

  # Runs the block while another thread sends this process `signal` every
  # millisecond, and returns how many arrived.
  def under_signals(signal)
    count = 0
    previous = trap(signal) { count += 1 }
    sender = Thread.new { loop { Process.kill(signal, Process.pid) && sleep(0.001) } }
    yield
    count
  ensure
    sender&.kill&.join
    trap(signal, previous || "DEFAULT")
  end

  def test_a_read_interrupted_by_signals_resumes_where_it_stopped
    skip "needs SIGUSR1" unless Signal.list.key?("USR1")

    path = write(xml: Workbook.shared_strings(*Array.new(200_000) { |i| "<si><t>string #{i}</t></si>" }))
    expected = Roo::Excelx.new(path).shared_strings.to_a
    Oracle.counts.clear
    strings = nil
    signals = under_signals("USR1") do
      workbook = Roo::Excelx.new(path)
      strings = workbook.shared_strings.to_a
    end

    assert_operator signals, :>, 0
    assert_equal 1, Oracle.counts[:native_shared_strings]
    assert_equal [expected.size, expected.hash], [strings.size, strings.hash], "the interrupted read differs"
  end

  def test_kill_switch_and_other_roo_versions_leave_shared_strings_to_roo
    path = write("<si><t>a</t></si>")
    Roo::Calamine.enabled = false
    Oracle.counts.clear

    assert_equal ["a"], Roo::Excelx.new(path).shared_strings.to_a
    assert_equal 0, Oracle.counts[:native_shared_strings]

    Roo::Calamine.enabled = true
    original = Roo::VERSION
    Roo.send(:remove_const, :VERSION)
    Roo.const_set(:VERSION, "4.1.0")

    assert_shared_strings_same_as_roo("roo 4.1.0", path, native: false)
  ensure
    Roo::Calamine.enabled = true
    if original
      Roo.send(:remove_const, :VERSION)
      Roo.const_set(:VERSION, original)
    end
  end
end
