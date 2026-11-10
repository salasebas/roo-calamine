# frozen_string_literal: true

# Compares plain roo, roo with roo-calamine, and calx (as a reference for a
# reader that skips roo's cell model) on generated workbooks.
#
#   cd bench && bundle install && bundle exec ruby run.rb [RUNS]
#
# Each run is a fresh process. The table shows the median wall time and peak
# RSS of RUNS runs (default 3). roo and roo-calamine must read the same values.
require "etc"
require "json"
require "open3"
require "rbconfig"

RUNS = Integer(ARGV.fetch(0, "3"))
LIBRARIES = %w[roo roo-calamine calx].freeze
SCENARIOS = {
  "stream" => "`Roo::Spreadsheet.open` + `each_row_streaming`",
  "parse" => "`parse(headers: true)`",
  "cell" => "10,000 random `cell(row, col)`"
}.freeze
FIXTURES = %w[mixed-100k.xlsx shared-strings-100k.xlsx small.xlsx].freeze

system(RbConfig.ruby, File.join(__dir__, "generate.rb"), exception: true)

def run(library, scenario, path)
  results = Array.new(RUNS) do
    out, status = Open3.capture2(RbConfig.ruby, File.join(__dir__, "scenario.rb"), library, scenario, path)
    raise "#{library} #{scenario} #{path} failed" unless status.success?

    JSON.parse(out.lines.last)
  end
  digests = results.map { |r| r["digest"] }.uniq
  raise "#{library} #{scenario} read different values across runs" unless digests.size == 1

  median = ->(key) { results.map { |r| r[key] }.sort[RUNS / 2] }
  { seconds: median.call("seconds"), peak_rss: median.call("peak_rss"), digest: digests.first }
end

def cpu
  if RUBY_PLATFORM.include?("darwin")
    `sysctl -n machdep.cpu.brand_string`.strip
  else
    File.read("/proc/cpuinfo")[/model name\s*:\s*(.+)/, 1]
  end
end

puts "Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}), #{`uname -sr`.strip}, #{cpu}, #{Etc.nprocessors} cores, " \
     "median of #{RUNS} runs\n\n"
puts "| File | Scenario | roo | roo + roo-calamine | speedup | calx (reference) |"
puts "|---|---|---:|---:|---:|---:|"
FIXTURES.each do |fixture|
  path = File.join(__dir__, "fixtures", fixture)
  SCENARIOS.each do |scenario, label|
    results = LIBRARIES.to_h { |library| [library, run(library, scenario, path)] }
    unless results["roo"][:digest] == results["roo-calamine"][:digest]
      raise "roo-calamine read different values than roo: #{fixture} #{scenario}"
    end

    cell = ->(r) { format("%.2f s, %d MB", r[:seconds], r[:peak_rss] / 1_048_576) }
    speedup = results["roo"][:seconds] / results["roo-calamine"][:seconds]
    puts "| #{fixture} | #{label} | #{cell.call(results['roo'])} | #{cell.call(results['roo-calamine'])} | " \
         "#{format('%.1f×', speedup)} | #{cell.call(results['calx'])} |"
  end
end
