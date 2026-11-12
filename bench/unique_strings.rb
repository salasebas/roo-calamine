# frozen_string_literal: true

require "json"
require "objspace"
require "roo/calamine"

mode = ARGV.shift
scenario = ARGV.shift if mode == "profile"
path = ARGV.shift || File.join(__dir__, "fixtures", "unique-strings-100k.xlsx")

def clock = Process.clock_gettime(Process::CLOCK_MONOTONIC)

def measure
  before = GC.stat
  started = clock
  result = yield
  [result, { seconds: clock - started, objects: GC.stat(:total_allocated_objects) - before[:total_allocated_objects] }]
end

case mode
when "profile"
  phases = {}
  native = Roo::Calamine::Native
  [[native.singleton_class, :shared_strings], [native::Store.singleton_class, :read],
   [native::Store, :bounds], [native::Store, :each_row]].each do |target, method|
    target.prepend(Module.new do
      define_method(method) do |*args, &block|
        result, timing = measure { super(*args, &block) }
        phases[method] = timing
        result
      end
    end)
  end
  GC.start
  ARGV.replace(["roo-calamine", scenario, path])
  _, timing = measure { load File.join(__dir__, "scenario.rb") }
  puts JSON.generate(timing.merge(phases:))
when "floor"
  strings = Roo::Excelx.new(path).shared_strings.to_a
  puts JSON.generate(strings: strings.size, string_bytes: strings.sum { ObjectSpace.memsize_of(_1) },
                     array_bytes: ObjectSpace.memsize_of(strings))
else
  abort "usage: unique_strings.rb (profile SCENARIO|floor) [FILE]"
end
