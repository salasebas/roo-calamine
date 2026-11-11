# frozen_string_literal: true

require "json"
require "roo/calamine"

mode, path = ARGV
abort "usage: each.rb (each|row|unchecked) FILE" unless %w[each row unchecked].include?(mode) && path

def clock = Process.clock_gettime(Process::CLOCK_MONOTONIC)

GC.start
started = clock
workbook = Roo::Excelx.new(path)
sheet = workbook.sheet_for(nil)
store = sheet.lazy_cells
last = workbook.last_row
loaded = clock

case mode
when "each"
  workbook.each { |row| row }
when "row"
  1.upto(last) { |line| workbook.row(line) }
when "unchecked"
  store.each_row(last, sheet.first_column, sheet.last_column) { true }
end

puts JSON.generate(mode:, load_seconds: loaded - started, rows_seconds: clock - loaded)
workbook.close
