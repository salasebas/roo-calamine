# frozen_string_literal: true

# Compares plain roo, roo with roo-calamine, and calx (a reader that skips
# roo's cell model, as a reference) on generated workbooks.
#
#   cd bench && bundle install && bundle exec ruby run.rb [RUNS]
#
# Each run is a fresh process. The table shows the median wall time and peak
# RSS of RUNS timed runs (default 3). Before timing, one run of roo and one of
# roo-calamine hash every value they read; the two hashes must match.
require "etc"
require "json"
require "open3"
require "rbconfig"

RUNS = Integer(ARGV.fetch(0, "3"))
LIBRARIES = %w[roo roo-calamine calx].freeze
SCENARIOS = {
  "stream" => "`Roo::Spreadsheet.open` + `each_row_streaming`",
  "parse" => "`parse(headers: true)`",
  "cell" => "10,000 random `cell(row, col)`",
  "values" => "values only: `Roo::Calamine.each_row_values`"
}.freeze
COMPARED = %w[roo roo-calamine].freeze
FIXTURES = %w[mixed-100k.xlsx shared-strings-100k.xlsx small.xlsx].freeze
MB = 1024 * 1024

system(RbConfig.ruby, File.join(__dir__, "generate.rb"), exception: true)

def scenario(*args)
  out, status = Open3.capture2(RbConfig.ruby, File.join(__dir__, "scenario.rb"), *args)
  raise "scenario.rb #{args.join(" ")} failed" unless status.success?

  JSON.parse(out.lines.last)
end

def timed(library, name, path)
  results = Array.new(RUNS) { scenario(library, name, path) }
  median = ->(key) { results.map { |r| r[key] }.sort[RUNS / 2] }
  { seconds: median.call("seconds"), peak_rss: median.call("peak_rss") }
end

def cpu
  return `sysctl -n machdep.cpu.brand_string`.strip if RUBY_PLATFORM.include?("darwin")

  File.read("/proc/cpuinfo")[/model name\s*:\s*(.+)/, 1]
end

def describe(result)
  format("%<seconds>.2f s, %<mb>d MB", seconds: result[:seconds], mb: result[:peak_rss] / MB)
end

puts "Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}), #{`uname -sr`.strip}, #{cpu}, #{Etc.nprocessors} cores, " \
     "median of #{RUNS} runs", ""
puts "| File | Scenario | roo | roo + roo-calamine | Speedup | calx (reference) |", "|---|---|---:|---:|---:|---:|"
FIXTURES.each do |fixture|
  path = File.join(__dir__, "fixtures", fixture)
  SCENARIOS.each do |name, label|
    digests = COMPARED.map { |library| scenario(library, name, path, "digest")["digest"] }
    raise "roo-calamine read different values than roo: #{fixture} #{name}" unless digests.uniq.size == 1

    roo, calamine, calx = LIBRARIES.map { |library| timed(library, name, path) }
    speedup = format("%.1f×", roo[:seconds] / calamine[:seconds])
    puts "| #{fixture} | #{label} | #{describe(roo)} | #{describe(calamine)} | #{speedup} | #{describe(calx)} |"
  end
end
