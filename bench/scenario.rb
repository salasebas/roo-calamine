# frozen_string_literal: true

# One benchmark run in a fresh process: bench/scenario.rb LIBRARY SCENARIO FILE.
# Prints JSON with the wall time, the peak RSS of this process, and a digest
# of every value read, so runs of roo and roo-calamine can be compared.
require "digest"
require "fiddle"
require "json"

library, scenario, path = ARGV
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

def roo_scenario(scenario, path, digest)
  workbook = Roo::Spreadsheet.open(path)
  case scenario
  when "stream"
    workbook.each_row_streaming { |row| row.each { |cell| digest << cell&.value.inspect } }
  when "parse"
    workbook.parse(headers: true).each { |row| digest << row.values.inspect }
  when "cell"
    random = Random.new(1)
    rows = workbook.last_row
    columns = workbook.last_column
    RANDOM_CELLS.times { digest << workbook.cell(random.rand(1..rows), random.rand(1..columns)).inspect }
  end
end

def calx_scenario(scenario, path, digest)
  case scenario
  when "stream"
    Calx.each_row(path) { |row| row.each { |value| digest << value.inspect } }
  when "parse"
    header, *rows = Calx.read(path)
    rows.each { |row| digest << header.zip(row).to_h.values.inspect }
  when "cell"
    rows = Calx.read(path)
    random = Random.new(1)
    RANDOM_CELLS.times do
      row = rows[random.rand(rows.size)]
      digest << row[random.rand(row.size)].inspect
    end
  end
end

GC.start
digest = Digest::SHA256.new
started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
library == "calx" ? calx_scenario(scenario, path, digest) : roo_scenario(scenario, path, digest)
seconds = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
puts JSON.generate(seconds:, peak_rss: peak_rss_bytes, digest: digest.hexdigest)
