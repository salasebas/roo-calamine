# frozen_string_literal: true

# Compares plain roo, roo with roo-calamine, and calx (a reader that skips
# roo's cell model, as a reference) on generated workbooks.
#
#   cd bench && bundle install && bundle exec ruby run.rb [RUNS] [LIBRARY...]
#
# Each run is a fresh process. The table shows the median wall time and peak
# RSS of the timed runs. Without RUNS, each scenario runs at least 3 and at
# most 10 times, stopping once it has run for 10 seconds, so a slow library
# gets 3 runs and a fast one 10. LIBRARY limits the timed runs to those
# libraries; the others show "-". Before timing, one run of roo and one of
# roo-calamine hash every value they read; the two hashes must match.
require "etc"
require "json"
require "open3"
require "rbconfig"

RUNS = Integer(ARGV.first, exception: false)
MIN_RUNS = 3
MAX_RUNS = 10
BUDGET_SECONDS = 10
RUN_COUNT = RUNS ? "#{RUNS} runs" : "#{MIN_RUNS}–#{MAX_RUNS} runs, stopping after #{BUDGET_SECONDS} s per scenario"
LIBRARIES = %w[roo roo-calamine calx].freeze
TIMED = (RUNS ? ARGV.drop(1) : ARGV).then { |names| names.empty? ? LIBRARIES : names }
raise "unknown library: #{(TIMED - LIBRARIES).join(", ")}" unless (TIMED - LIBRARIES).empty?

SCENARIOS = {
  "stream" => "`Roo::Spreadsheet.open` + `each_row_streaming`",
  "parse" => "`parse(headers: true)`",
  "each" => "`each` (one Array of values per row)",
  "cell" => "10,000 random `cell(row, col)`",
  "csv" => "`to_csv`",
  "values" => "values only: `Roo::Calamine.each_row_values`",
  "yaml" => "`to_yaml`",
  "xml" => "`to_xml`",
  "matrix" => "`to_matrix`"
}.freeze
CALX_SCENARIOS = %w[stream parse each cell csv values].freeze
COMPARED = %w[roo roo-calamine].freeze
FIXTURES = %w[mixed-100k.xlsx shared-strings-100k.xlsx unique-strings-100k.xlsx small.xlsx].freeze
MB = 1024 * 1024

system(RbConfig.ruby, File.join(__dir__, "generate.rb"), exception: true)

def scenario(*args)
  out, status = Open3.capture2(RbConfig.ruby, File.join(__dir__, "scenario.rb"), *args)
  raise "scenario.rb #{args.join(" ")} failed" unless status.success?

  JSON.parse(out.lines.last)
end

def more_runs?(count, started)
  return count < RUNS if RUNS
  return true if count < MIN_RUNS

  count < MAX_RUNS && Process.clock_gettime(Process::CLOCK_MONOTONIC) - started < BUDGET_SECONDS
end

def median(values)
  sorted = values.sort
  middle = sorted.size / 2
  sorted.size.odd? ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2.0
end

def timed(library, name, path)
  started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  results = []
  results << scenario(library, name, path) while more_runs?(results.size, started)
  { seconds: median(results.map { |r| r["seconds"] }), peak_rss: median(results.map { |r| r["peak_rss"] }) }
end

def timed?(library, name)
  TIMED.include?(library) && (library != "calx" || CALX_SCENARIOS.include?(name))
end

def cpu
  return `sysctl -n machdep.cpu.brand_string`.strip if RUBY_PLATFORM.include?("darwin")

  File.read("/proc/cpuinfo")[/model name\s*:\s*(.+)/, 1]
end

def describe(result)
  return "-" unless result

  seconds = result[:seconds]
  time = seconds < 0.1 ? format("%.1f ms", seconds * 1000) : format("%.2f s", seconds)
  "#{time}, #{result[:peak_rss].to_i / MB} MB"
end

puts "Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}), #{`uname -sr`.strip}, #{cpu}, #{Etc.nprocessors} cores, " \
     "median of #{RUN_COUNT}", ""
puts "| File | Scenario | roo | roo + roo-calamine | Speedup | calx (reference) |", "|---|---|---:|---:|---:|---:|"
FIXTURES.each do |fixture|
  path = File.join(__dir__, "fixtures", fixture)
  SCENARIOS.each do |name, label|
    digests = COMPARED.map { |library| scenario(library, name, path, "digest")["digest"] }
    raise "roo-calamine read different values than roo: #{fixture} #{name}" unless digests.uniq.size == 1

    roo, calamine, calx = LIBRARIES.map { |library| timed(library, name, path) if timed?(library, name) }
    speedup = roo && calamine ? format("%.1f×", roo[:seconds] / calamine[:seconds]) : "-"
    puts "| #{fixture} | #{label} | #{describe(roo)} | #{describe(calamine)} | #{speedup} | #{describe(calx)} |"
  end
end
