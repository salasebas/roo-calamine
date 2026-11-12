# frozen_string_literal: true

require "json"
require "objspace"
require "roo/calamine"

path, mode = ARGV
abort "usage: ruby bench/fork_probe.rb FILE warm|sparse" unless path && %w[warm sparse].include?(mode)
clock = -> { Process.clock_gettime(Process::CLOCK_MONOTONIC) }
random = Random.new(1)
coordinates = Array.new(10_000) { [random.rand(1..100), random.rand(1..20)] }
start = clock.call
workbook = Roo::Excelx.new(path)
first = workbook.cell(1, 1)
loaded = clock.call
if mode == "warm"
  dimensions = [workbook.last_row, workbook.last_column]
  first_reads = coordinates.map { |row, column| workbook.cell(row, column) }
  queried = clock.call
  repeated = coordinates.map { |row, column| workbook.cell(row, column) }
  finished = clock.call
  abort "warm reads changed" unless first_reads == repeated
  puts JSON.generate(open_first: loaded - start, first_reads: queried - loaded,
                     repeat_reads: finished - queried, dimensions:, first:)
else
  sheet = workbook.sheet_for(nil)
  store = sheet.lazy_cells
  memory = ObjectSpace.memsize_of(store)
  far = workbook.cell(1_048_576, 16_384)
  count = sheet.cells.size
  puts JSON.generate(open_first: loaded - start, store_bytes: memory, far:, materialized_cells: count,
                     last_row: workbook.last_row, last_column: workbook.last_column, first:)
end
workbook.close
