# frozen_string_literal: true

require "json"
require "calx"

path = ARGV.fetch(0)
random = Random.new(1)
coordinates = Array.new(10_000) { [random.rand(1..100), random.rand(1..20)] }
clock = -> { Process.clock_gettime(Process::CLOCK_MONOTONIC) }
started = clock.call
rows = Calx.read(path)
loaded = clock.call
first = coordinates.map { |row, column| rows[row - 1][column - 1] }
queried = clock.call
repeated = coordinates.map { |row, column| rows[row - 1][column - 1] }
finished = clock.call
abort "warm reference reads changed" unless first == repeated
puts JSON.generate(load: loaded - started, first_reads: queried - loaded, repeat_reads: finished - queried)
