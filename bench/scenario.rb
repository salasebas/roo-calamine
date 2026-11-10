# frozen_string_literal: true

# One benchmark run in a fresh process:
#
#   bench/scenario.rb LIBRARY SCENARIO FILE [digest]
#
# Prints JSON with the wall time and the peak RSS of this process. With
# `digest`, it also hashes every value read, so the values roo and roo-calamine
# return can be compared; that run is not timed in the table.
require "digest"
require "fiddle"
require "json"

library, scenario, path, mode = ARGV
DIGESTING = mode == "digest"
case library
when "roo" then require "roo"
when "roo-calamine" then require "roo/calamine"
when "calx" then require "calx"
end

def peak_rss_bytes
  rusage = Fiddle::Pointer.malloc(256, Fiddle::RUBY_FREE)
  getrusage = Fiddle::Function.new(Fiddle::Handle::DEFAULT["getrusage"], [Fiddle::TYPE_INT, Fiddle::TYPE_VOIDP],
                                   Fiddle::TYPE_INT)
  getrusage.call(0, rusage)
  maxrss = rusage[32, 8].unpack1("q")
  RUBY_PLATFORM.include?("darwin") ? maxrss : maxrss * 1024
end

RANDOM_CELLS = 10_000

def value(object)
  DIGESTING ? object.inspect : object
end

def roo_scenario(scenario, path, digest)
  workbook = Roo::Spreadsheet.open(path)
  case scenario
  when "stream"
    workbook.each_row_streaming { |row| row.each { |cell| digest << value(cell&.value) } }
  when "parse"
    workbook.parse(headers: true).each { |row| digest << value(row.values) }
  when "cell"
    random = Random.new(1)
    rows = workbook.last_row
    columns = workbook.last_column
    RANDOM_CELLS.times { digest << value(workbook.cell(random.rand(1..rows), random.rand(1..columns))) }
  end
end

def values_scenario(library, path, digest)
  if library == "roo-calamine"
    Roo::Calamine.each_row_values(path) { |row| row.each { |cell_value| digest << value(cell_value) } }
  else
    Roo::Spreadsheet.open(path).each_row_streaming(pad_cells: true) do |row|
      row.each { |cell| digest << value(cell&.value) }
    end
  end
end

def calx_scenario(scenario, path, digest)
  case scenario
  when "stream", "values"
    Calx.each_row(path) { |row| row.each { |value| digest << value(value) } }
  when "parse"
    header, *rows = Calx.read(path)
    rows.each { |row| digest << value(header.zip(row).to_h.values) }
  when "cell"
    rows = Calx.read(path)
    random = Random.new(1)
    RANDOM_CELLS.times do
      row = rows[random.rand(rows.size)]
      digest << value(row[random.rand(row.size)])
    end
  end
end

# Timed runs only touch each value; `<<` on this sink does nothing.
class Sink
  def <<(_value) = self
  def hexdigest = nil
end

GC.start
digest = DIGESTING ? Digest::SHA256.new : Sink.new
started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
if library == "calx"
  calx_scenario(scenario, path, digest)
elsif scenario == "values"
  values_scenario(library, path, digest)
else
  roo_scenario(scenario, path, digest)
end
seconds = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
puts JSON.generate(seconds:, peak_rss: peak_rss_bytes, digest: digest.hexdigest)
