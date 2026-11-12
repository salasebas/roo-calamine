# frozen_string_literal: true

require "json"
require "roo/calamine"

path = ARGV.fetch(0)
random = Random.new(1)
coordinates = Array.new(10_000) { [random.rand(1..100), random.rand(1..20)] }
clock = -> { Process.clock_gettime(Process::CLOCK_MONOTONIC) }
started = clock.call
workbook = Roo::Excelx.new(path)
store = workbook.sheet_for(nil).lazy_cells
parsed = clock.call
cells = coordinates.map { |row, column| store.cell_at(row, column) }
built = clock.call
values = cells.map(&:value)
valued = clock.call
native = coordinates.map { |row, column| store.cell_at(row, column).value }
repeated = clock.call
public_values = coordinates.map { |row, column| workbook.cell(row, column) }
finished = clock.call
abort "component values differ" unless values == native && values == public_values
puts JSON.generate(open_and_records: parsed - started, first_native_cells: built - parsed,
                   cell_values: valued - built, warm_native_values: repeated - valued,
                   warm_public_values: finished - repeated)
workbook.close
