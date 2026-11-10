# frozen_string_literal: true

# Measurements behind the optimization notes in the README.
#
#   cd bench && bundle exec ruby optimizations.rb (gvl|strings|lazy) LIBRARY
#
# LIBRARY is roo or roo-calamine. Run generate.rb (or run.rb) first.
require "json"

check, library = ARGV
library == "roo" ? require("roo") : require("roo/calamine")

FIXTURES = File.join(__dir__, "fixtures")

def clock
  Process.clock_gettime(Process::CLOCK_MONOTONIC)
end

def timed
  started = clock
  yield
  clock - started
end

# A second Ruby thread runs a busy loop while the main thread reads a sheet
# that is already unzipped. Reports how long the read takes and how much the
# other thread got done, as a share of what it does alone in the same time.
def gvl(path)
  workbook = Roo::Excelx.new(path)
  ticks = 0
  running = true
  worker = Thread.new { ticks += 1 while running }
  seconds = timed { workbook.sheet_for(nil).cells }
  running = false
  worker.join
  alone = 0
  stop = clock + seconds
  alone += 1 while clock < stop
  { cells_seconds: seconds.round(2), other_thread_share: (ticks.to_f / alone).round(2) }
end

# Objects allocated and time to read every cell of a sheet full of repeated
# shared strings.
def strings(path)
  GC.start
  before = GC.stat(:total_allocated_objects)
  seconds = timed { Roo::Excelx.new(path).sheet_for(nil).cells }
  { cells_seconds: seconds.round(2), allocated_objects: GC.stat(:total_allocated_objects) - before }
end

# Rows roo never hands to the block are never turned into cells.
def lazy(path)
  {
    "all rows" => timed { Roo::Excelx.new(path).each_row_streaming { |row| row } }.round(2),
    "offset: 90_000" => timed { Roo::Excelx.new(path).each_row_streaming(offset: 90_000) { |row| row } }.round(2),
    "max_rows: 100" => timed { Roo::Excelx.new(path).each_row_streaming(max_rows: 100) { |row| row } }.round(3)
  }
end

result =
  case check
  when "gvl" then gvl(File.join(FIXTURES, "mixed-100k.xlsx"))
  when "strings" then strings(File.join(FIXTURES, "shared-strings-100k.xlsx"))
  when "lazy" then lazy(File.join(FIXTURES, "mixed-100k.xlsx"))
  else abort "usage: optimizations.rb (gvl|strings|lazy) (roo|roo-calamine)"
  end
puts JSON.generate({ check:, library: }.merge(result))
