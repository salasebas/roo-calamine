# frozen_string_literal: true

# Run on JRuby against the installed gem: `ruby test/jruby_smoke.rb`.
# roo-calamine must load, stay inert, and leave roo reading workbooks itself.
require "tmpdir"
require "roo/calamine"
require_relative "support/workbook"

def check(condition, message)
  abort "FAIL: #{message}" unless condition
  puts "ok: #{message}"
end

check RUBY_ENGINE == "jruby", "running on JRuby (#{RUBY_ENGINE} #{RUBY_ENGINE_VERSION})"
check !Roo::Calamine.available?, "the native extension is not available"
check !Roo::Calamine.enabled?, "roo-calamine is disabled"
check Roo::Excelx::SheetDoc.ancestors.none? { |a| a.name.to_s.start_with?("Roo::Calamine") },
      "Roo::Excelx::SheetDoc is not patched"

Dir.mktmpdir do |dir|
  rows = %(<row r="1"><c r="A1" t="inlineStr"><is><t>name</t></is></c><c r="B1"><v>1.0</v></c></row>)
  path = Workbook.write(dir, "smoke", sheets: { "Sheet1" => Workbook.sheet(rows) })
  workbook = Roo::Spreadsheet.open(path)
  check workbook.cell(1, 1) == "name", "roo reads a string cell"
  check workbook.cell(1, 2).inspect == "1.0", "roo reads 1.0 as the Float roo returns"
  check workbook.each_row_streaming.map { |row| row.map(&:value) } == [["name", 1.0]], "each_row_streaming works"
end
