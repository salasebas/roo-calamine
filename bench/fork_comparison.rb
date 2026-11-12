# frozen_string_literal: true

require "json"
require "open3"
require "rbconfig"

baseline, output, runs, control = ARGV
abort "usage: ruby bench/fork_comparison.rb BASELINE_LIB OUTPUT [RUNS]" unless baseline && output
runs = Integer(runs || 5)
paths = { "baseline" => File.expand_path(baseline), "candidate" => File.expand_path("../lib", __dir__) }
cases = {
  "small.xlsx" => %w[open cell celltype stream stream-max-rows each csv],
  "mixed-100k.xlsx" => %w[cell celltype stream stream-max-rows each csv],
  "shared-strings-100k.xlsx" => %w[cell stream],
  "unique-strings-100k.xlsx" => %w[cell stream]
}.freeze
paths["control"] = File.expand_path(control) if control
def measurement(command)
  text, status = Open3.capture2e(*command)
  abort "#{command.join(" ")}: #{text}" unless status.success?
  JSON.parse(text)
end

results = []
oracle = {}
cases.each do |file, scenarios|
  scenarios.each do |scenario|
    command = [RbConfig.ruby, File.join(__dir__, "scenario.rb"), "roo-calamine", scenario,
               File.join(__dir__, "fixtures", file)]
    digests = paths.values.map do |path|
      measurement([command.first, "-I", path, *command.drop(1), "digest"]).fetch("digest")
    end
    roo_command = command.dup
    roo_command[2] = "roo"
    digests << measurement([*roo_command, "digest"]).fetch("digest")
    abort "digest mismatch: #{file} #{scenario}" unless digests.uniq.one?
    oracle["#{file}:#{scenario}"] = digests.first

    runs.times do |run|
      order = run.even? ? paths.to_a : paths.to_a.reverse
      order.each do |label, path|
        result = measurement([command.first, "-I", path, *command.drop(1)])
        results << { backend: label, file:, scenario:, run:, **result }
      end
    end
    File.write(output, JSON.pretty_generate({ ruby: RUBY_DESCRIPTION, runs:, oracle:, results: }))
    warn "#{file} #{scenario}: digests match, #{runs} pairs"
  end
end
