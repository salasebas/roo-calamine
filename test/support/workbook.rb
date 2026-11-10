# frozen_string_literal: true

require "tmpdir"
require "zip"

# Writes minimal .xlsx files from raw XML parts, so a test can put exactly the
# markup it is about into a worksheet.
module Workbook
  MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
  REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

  # Cell style indexes used by the tests: 0 General, 1 a date, 2 a date with a
  # time, 3 a time, 4 a percentage, 5 "0.00", 6 a custom date format, 7 text
  # ("@"), 8 an undefined number format id.
  STYLES = <<~XML.freeze
    <styleSheet xmlns="#{MAIN}">
      <numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\\-mm\\-dd hh:mm"/></numFmts>
      <fonts count="2"><font/><font><b/></font></fonts>
      <cellXfs count="9">
        <xf numFmtId="0" fontId="0"/><xf numFmtId="14" fontId="0"/><xf numFmtId="22" fontId="0"/>
        <xf numFmtId="21" fontId="0"/><xf numFmtId="10" fontId="0"/><xf numFmtId="2" fontId="1"/>
        <xf numFmtId="164" fontId="0"/><xf numFmtId="49" fontId="0"/><xf numFmtId="200" fontId="0"/>
      </cellXfs>
    </styleSheet>
  XML

  module_function

  def sheet(rows, extra: "")
    %(<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="#{MAIN}" xmlns:r="#{REL}"><sheetData>#{rows}</sheetData>#{extra}</worksheet>)
  end

  def shared_strings(*items)
    %(<sst xmlns="#{MAIN}" count="#{items.size}" uniqueCount="#{items.size}">#{items.join}</sst>)
  end

  # sheets: { "Name" => worksheet_xml }; sheet_rels: { "Name" => rels_xml }
  def write(dir, name, sheets:, styles: STYLES, shared_strings: nil, sheet_rels: {})
    path = File.join(dir, "#{name}.xlsx")
    Zip::File.open(path, create: true) do |zip|
      add = ->(entry, xml) { zip.get_output_stream(entry) { |io| io.write(xml) } }
      add.call("[Content_Types].xml", content_types(sheets.size))
      add.call("_rels/.rels",
               %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="#{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>))
      add.call("xl/workbook.xml", workbook(sheets.keys))
      add.call("xl/_rels/workbook.xml.rels", workbook_rels(sheets.size))
      add.call("xl/styles.xml", styles) if styles
      add.call("xl/sharedStrings.xml", shared_strings) if shared_strings
      sheets.each_with_index do |(sheet_name, xml), index|
        add.call("xl/worksheets/sheet#{index + 1}.xml", xml)
        rels = sheet_rels[sheet_name]
        add.call("xl/worksheets/_rels/sheet#{index + 1}.xml.rels", rels) if rels
      end
    end
    path
  end

  def content_types(count)
    overrides = (1..count).map do |i|
      %(<Override PartName="/xl/worksheets/sheet#{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>)
    end
    %(<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/>#{overrides.join}</Types>)
  end

  def workbook(names)
    sheets = names.each_with_index.map do |name, i|
      %(<sheet name="#{name}" sheetId="#{i + 1}" r:id="rId#{i + 1}"/>)
    end
    %(<workbook xmlns="#{MAIN}" xmlns:r="#{REL}"><sheets>#{sheets.join}</sheets></workbook>)
  end

  def workbook_rels(count)
    rels = (1..count).map do |i|
      %(<Relationship Id="rId#{i}" Type="#{REL}/worksheet" Target="worksheets/sheet#{i}.xml"/>)
    end
    %(<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">#{rels.join}</Relationships>)
  end
end
