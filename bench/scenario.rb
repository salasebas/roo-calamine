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

def random_cells(workbook, digest)
  random = Random.new(1)
  rows = workbook.last_row
  columns = workbook.last_column
  RANDOM_CELLS.times { digest << value(workbook.cell(random.rand(1..rows), random.rand(1..columns))) }
end

ROO_SCENARIOS = {
  "stream" => ->(workbook, digest) { workbook.each_row_streaming { |row| row.each { digest << value(_1&.value) } } },
  "parse" => ->(workbook, digest) { workbook.parse(headers: true).each { |row| digest << value(row.values) } },
  "each" => ->(workbook, digest) { workbook.each { |row| digest << value(row) } },
  "cell" => method(:random_cells),
  "csv" => ->(workbook, digest) { digest << workbook.to_csv },
  "yaml" => ->(workbook, digest) { digest << workbook.to_yaml },
  "xml" => ->(workbook, digest) { digest << workbook.to_xml },
  "matrix" => ->(workbook, digest) { digest << value(workbook.to_matrix) }
}.freeze

def roo_scenario(scenario, path, digest)
  ROO_SCENARIOS.fetch(scenario).call(Roo::Spreadsheet.open(path), digest)
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

def calx_random_cells(path, digest)
  rows = Calx.read(path)
  random = Random.new(1)
  RANDOM_CELLS.times do
    row = rows[random.rand(rows.size)]
    digest << value(row[random.rand(row.size)])
  end
end

def calx_parse(path, digest)
  header, *rows = Calx.read(path)
  rows.each { |row| digest << value(header.zip(row).to_h.values) }
end

CALX_VALUES = ->(path, digest) { Calx.each_row(path) { |row| row.each { |cell_value| digest << value(cell_value) } } }
CALX_SCENARIOS = {
  "stream" => CALX_VALUES,
  "values" => CALX_VALUES,
  "each" => ->(path, digest) { Calx.each_row(path) { |row| digest << value(row) } },
  "parse" => method(:calx_parse),
  "cell" => method(:calx_random_cells),
  "csv" => ->(path, digest) { Calx.each_row(path) { |row| digest << row.join(",") } }
}.freeze

# Timed runs only touch each value; `<<` on this sink does nothing.
class Sink
  def <<(_value) = self
  def hexdigest = nil
end

GC.start
digest = DIGESTING ? Digest::SHA256.new : Sink.new
started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
if library == "calx"
  CALX_SCENARIOS.fetch(scenario).call(path, digest)
elsif scenario == "values"
  values_scenario(library, path, digest)
else
  roo_scenario(scenario, path, digest)
end
seconds = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
puts JSON.generate(seconds:, peak_rss: peak_rss_bytes, digest: digest.hexdigest)
