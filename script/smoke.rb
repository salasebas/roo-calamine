# frozen_string_literal: true

# Run against an installed roo-calamine gem: `ruby script/smoke.rb`.
# Writes a small workbook, reads it with plain roo code, and checks that the
# native reader served the read and returned roo's values.
require "tmpdir"
require "roo/calamine"
require "zip"

def check(condition, message)
  abort "FAIL: #{message}" unless condition
  puts "ok: #{message}"
end

main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
package = "http://schemas.openxmlformats.org/package/2006"
parts = {
  "[Content_Types].xml" => %(<Types xmlns="#{package}/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>),
  "_rels/.rels" => %(<Relationships xmlns="#{package}/relationships"><Relationship Id="rId1" Type="#{rel}/officeDocument" Target="xl/workbook.xml"/></Relationships>),
  "xl/workbook.xml" => %(<workbook xmlns="#{main}" xmlns:r="#{rel}"><sheets><sheet name="Smoke" sheetId="1" r:id="rId1"/></sheets></workbook>),
  "xl/_rels/workbook.xml.rels" => %(<Relationships xmlns="#{package}/relationships"><Relationship Id="rId1" Type="#{rel}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>),
  "xl/styles.xml" => %(<styleSheet xmlns="#{main}"><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14"/></cellXfs></styleSheet>),
  "xl/worksheets/sheet1.xml" => %(<worksheet xmlns="#{main}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>name</t></is></c>) +
                                %(<c r="B1"><v>1.0</v></c><c r="C1"><v>2</v></c><c r="D1" s="1"><v>45000</v></c></row></sheetData></worksheet>)
}

native_reads = 0
Roo::Calamine::Native::Reader.singleton_class.prepend(Module.new do
  define_method(:new) do |*args|
    native_reads += 1
    super(*args)
  end
end)

check Roo::Calamine.enabled?, "roo-calamine #{Roo::Calamine::VERSION} is enabled on #{RUBY_PLATFORM}, Ruby #{RUBY_VERSION}"
Dir.mktmpdir do |dir|
  path = File.join(dir, "smoke.xlsx")
  Zip::File.open(path, create: true) { |zip| parts.each { |name, xml| zip.get_output_stream(name) { |io| io.write(xml) } } }

  values = Roo::Spreadsheet.open(path).row(1).map(&:inspect)
  check values == ["\"name\"", "1.0", "2", "#<Date: 2023-03-15 ((2460019j,0s,0n),+0s,2299161j)>"],
        "Roo::Spreadsheet.open reads #{values.join(", ")}"
  streamed = Roo::Excelx.new(path).each_row_streaming.map { |row| row.map(&:value) }
  check streamed.flatten.size == 4, "each_row_streaming yields the row"
  check native_reads == 2, "both reads went through the native reader"
end
