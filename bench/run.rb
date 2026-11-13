# frozen_string_literal: true

# Compares plain roo, roo with roo-turbo, and two readers that skip roo's
# cell model, calx and xsv (as references), on generated workbooks.
#
#   cd bench && bundle install && bundle exec ruby run.rb [OPTIONS] [RUNS] [LIBRARY...]
#
# Each run is a fresh process. The table shows the median wall time and peak
# RSS of the timed runs. Without RUNS, each scenario runs at least 3 and at
# most 10 times, stopping once it has run for 10 seconds, so a slow library
# gets 3 runs and a fast one 10. LIBRARY limits the timed runs to those
# libraries; the others show "-". Before timing, one run of roo and one of
# roo-turbo hash every value they read; the two hashes must match.
#
# Without options, run.rb times the default table. --scenario, --group, and
# --fixture select other rows, and --all selects every row. --profile runs one
# scenario under vernier instead.
require "etc"
require "json"
require "open3"
require "optparse"
require "rbconfig"

Scenario = Struct.new(:group, :label, :fixtures)

DEFAULT_FIXTURES = %w[mixed-100k.xlsx shared-strings-100k.xlsx unique-strings-100k.xlsx small.xlsx].freeze
FEATURE_FIXTURES = %w[mixed-10k.xlsx wide.xlsx multi-sheet.xlsx links-merges-10k.xlsx comments-formulas-10k.xlsx
                      dates-10k.xlsx rich-text-10k.xlsx number-text-10k.xlsx fallback-10k.xlsx
                      fallback-dtd-10k.xlsx].freeze
FIXTURES = [*DEFAULT_FIXTURES, *FEATURE_FIXTURES, "large-1m.xlsx", "csv-10k.csv"].freeze
CSV_FIXTURE = %w[csv-10k.csv].freeze
SMALL_SHEETS = %w[mixed-10k.xlsx small.xlsx].freeze
LINKS = %w[links-merges-10k.xlsx].freeze
COMMENTS_FORMULAS = %w[comments-formulas-10k.xlsx].freeze

SCENARIOS = {
  "stream" => Scenario.new("default", "`Roo::Spreadsheet.open` + `each_row_streaming`",
                           [*DEFAULT_FIXTURES, *FEATURE_FIXTURES, "large-1m.xlsx"]),
  "parse" => Scenario.new("default", "`parse(headers: true)`", DEFAULT_FIXTURES),
  "each" => Scenario.new("default", "`each` (one Array of values per row)",
                         [*DEFAULT_FIXTURES, *FEATURE_FIXTURES, *CSV_FIXTURE]),
  "cell" => Scenario.new("default", "10,000 random `cell(row, col)`", [*DEFAULT_FIXTURES, *CSV_FIXTURE]),
  "csv" => Scenario.new("default", "`to_csv`", [*DEFAULT_FIXTURES, *FEATURE_FIXTURES, *CSV_FIXTURE]),
  "values" => Scenario.new("default", "values only: `Roo::Turbo.each_row_values`",
                           [*DEFAULT_FIXTURES, "large-1m.xlsx"]),
  "yaml" => Scenario.new("default", "`to_yaml`", [*DEFAULT_FIXTURES, "dates-10k.xlsx"]),
  "xml" => Scenario.new("default", "`to_xml`", [*DEFAULT_FIXTURES, "dates-10k.xlsx"]),
  "matrix" => Scenario.new("default", "`to_matrix`", [*DEFAULT_FIXTURES, "dates-10k.xlsx"]),

  "open" => Scenario.new("open", "`Roo::Excelx.new(path)`, no read", %w[mixed-100k.xlsx multi-sheet.xlsx small.xlsx]),
  "open-spreadsheet" => Scenario.new("open", "`Roo::Spreadsheet.open(path)`, no read", %w[mixed-100k.xlsx small.xlsx]),
  "open-stringio" => Scenario.new("open", "`Roo::Excelx.new(StringIO)`, no read", %w[mixed-100k.xlsx small.xlsx]),
  "open-file" => Scenario.new("open", "`Roo::Excelx.new(File)`, no read", %w[mixed-100k.xlsx small.xlsx]),

  "row" => Scenario.new("rows", "`row(n)` over every row", %w[mixed-100k.xlsx wide.xlsx small.xlsx]),
  "column" => Scenario.new("rows", "`column(n)` over every column", %w[mixed-100k.xlsx wide.xlsx small.xlsx]),
  "bounds" => Scenario.new("rows", "`first_row` … `last_column`, `*_column_as_letter`", %w[mixed-100k.xlsx small.xlsx]),
  "find" => Scenario.new("rows", "`find(:all, conditions:)`", %w[mixed-100k.xlsx small.xlsx]),
  "row_with" => Scenario.new("rows", "`row_with(headers)`", %w[mixed-100k.xlsx small.xlsx]),

  "celltype" => Scenario.new("single-cell", "10,000 random `celltype`", [*SMALL_SHEETS, "dates-10k.xlsx"]),
  "formula" => Scenario.new("single-cell", "10,000 random `formula`", [*SMALL_SHEETS, *COMMENTS_FORMULAS]),
  "is-formula" => Scenario.new("single-cell", "10,000 random `formula?`", [*SMALL_SHEETS, *COMMENTS_FORMULAS]),
  "formulas" => Scenario.new("single-cell", "`formulas`", [*SMALL_SHEETS, *COMMENTS_FORMULAS]),
  "excelx_type" => Scenario.new("single-cell", "10,000 random `excelx_type`", [*SMALL_SHEETS, "dates-10k.xlsx"]),
  "excelx_value" => Scenario.new("single-cell", "10,000 random `excelx_value`", [*SMALL_SHEETS, "dates-10k.xlsx"]),
  "formatted_value" => Scenario.new("single-cell", "10,000 random `formatted_value`",
                                    [*SMALL_SHEETS, "dates-10k.xlsx"]),
  "excelx_format" => Scenario.new("single-cell", "10,000 random `excelx_format`", [*SMALL_SHEETS, "dates-10k.xlsx"]),
  "font" => Scenario.new("single-cell", "10,000 random `font`", SMALL_SHEETS),
  "is-empty" => Scenario.new("single-cell", "10,000 random `empty?`", SMALL_SHEETS),
  "hyperlink" => Scenario.new("single-cell", "10,000 random `hyperlink`", [*SMALL_SHEETS, *LINKS]),
  "is-hyperlink" => Scenario.new("single-cell", "10,000 random `hyperlink?`", [*SMALL_SHEETS, *LINKS]),
  "comment" => Scenario.new("single-cell", "10,000 random `comment`", [*SMALL_SHEETS, *COMMENTS_FORMULAS]),
  "is-comment" => Scenario.new("single-cell", "10,000 random `comment?`", [*SMALL_SHEETS, *COMMENTS_FORMULAS]),

  "each-headers" => Scenario.new("iteration", "`each(headers: true)`", %w[mixed-100k.xlsx]),
  "each-header-search" => Scenario.new("iteration", "`each(header_search:)`", %w[mixed-100k.xlsx]),
  "each-query" => Scenario.new("iteration", "`each(key: \"header\", …)`", %w[mixed-100k.xlsx]),
  "each-clean" => Scenario.new("iteration", "`each(clean: true, headers: true)`", %w[mixed-100k.xlsx]),
  "each_with_pagename" => Scenario.new("iteration", "`each_with_pagename` + `each`", %w[multi-sheet.xlsx small.xlsx]),
  "cell-then-clean" => Scenario.new("iteration", "one `cell`, then `each(clean: true, headers: true)`",
                                    %w[mixed-100k.xlsx]),
  "parse-block" => Scenario.new("iteration", "`parse(headers: true) { }`", %w[mixed-100k.xlsx]),
  "stream-offset" => Scenario.new("iteration", "`each_row_streaming(offset: 5000)`", %w[mixed-100k.xlsx]),
  "stream-max-rows" => Scenario.new("iteration", "`each_row_streaming(max_rows: 1000)`", %w[mixed-100k.xlsx]),
  "stream-pad" => Scenario.new("iteration", "`each_row_streaming(pad_cells: true)`", %w[mixed-100k.xlsx]),
  "stream-stringio" => Scenario.new("iteration", "`each_row_streaming` from a `StringIO`", %w[mixed-100k.xlsx]),

  "cells" => Scenario.new("builders", "`sheet_for(...).cells`", %w[mixed-100k.xlsx small.xlsx]),
  "to_s" => Scenario.new("builders", "`to_s`", SMALL_SHEETS),

  "set-each" => Scenario.new("writes", "1,000 `set`, then `each`", %w[mixed-100k.xlsx small.xlsx]),
  "reload" => Scenario.new("writes", "1,000 `set`, `reload`, then `each`", %w[mixed-100k.xlsx small.xlsx]),

  "sheets" => Scenario.new("metadata", "`sheets`", %w[multi-sheet.xlsx small.xlsx]),
  "sheet" => Scenario.new("metadata", "`sheet(n)` + one `cell` per sheet", %w[multi-sheet.xlsx small.xlsx]),
  "dimensions" => Scenario.new("metadata", "`dimensions` of every sheet", %w[multi-sheet.xlsx small.xlsx]),
  "hyperlinks" => Scenario.new("metadata", "`sheet_for(...).hyperlinks`", LINKS),
  "comments" => Scenario.new("metadata", "`comments`", COMMENTS_FORMULAS),
  "labels" => Scenario.new("metadata", "`labels`", COMMENTS_FORMULAS),
  "images" => Scenario.new("metadata", "`images`", COMMENTS_FORMULAS),

  "expand_merged_ranges" => Scenario.new("options", "`expand_merged_ranges: true` + `each`", LINKS),
  "disable_html_wrapper" => Scenario.new("options", "`disable_html_wrapper: true` + `each`", %w[rich-text-10k.xlsx]),
  "cell_max" => Scenario.new("options", "`cell_max:` + `each`", %w[mixed-100k.xlsx]),
  "no_hyperlinks" => Scenario.new("options", "`no_hyperlinks: true` + `each`", LINKS)
}.freeze
GROUPS = SCENARIOS.values.map(&:group).uniq.freeze
LIBRARIES = %w[roo roo-turbo calx xsv].freeze
REFERENCES = %w[calx xsv].freeze
REFERENCE_SCENARIOS = %w[stream parse each cell csv values].freeze
COMPARED = %w[roo roo-turbo].freeze
MIN_RUNS = 3
MAX_RUNS = 10
BUDGET_SECONDS = 10
FLAG_FASTER_THAN_ROO = 5
FLAG_SLOWER_THAN_CALX = 1.2
MB = 1024 * 1024

def names(list, known, kind)
  unknown = list - known
  raise OptionParser::InvalidArgument, "unknown #{kind}: #{unknown.join(", ")}" unless unknown.empty?

  list
end

def fixture_name(name)
  FIXTURES.find { |fixture| [fixture, File.basename(fixture, ".*")].include?(name) } or
    raise OptionParser::InvalidArgument, "unknown fixture: #{name}"
end

options = {}
parser = OptionParser.new do |opts|
  opts.banner = "Usage: run.rb [OPTIONS] [RUNS] [LIBRARY...]\n       run.rb --profile SCENARIO FIXTURE [LIBRARY]"
  opts.on("--scenario NAMES", Array, "Only these scenarios") do |list|
    options[:scenarios] = names(list, SCENARIOS.keys, "scenario")
  end
  opts.on("--group NAMES", Array, "Only these groups: #{GROUPS.join(", ")}") do |list|
    options[:groups] = names(list, GROUPS, "group")
  end
  opts.on("--fixture NAMES", Array, "Only these fixtures") { |list| options[:fixtures] = list.map { fixture_name(_1) } }
  opts.on("--all", "Every scenario on every fixture it applies to") { options[:all] = true }
  opts.on("--profile", "Profile one scenario with vernier") { options[:profile] = true }
end
ARGS = parser.parse(ARGV)

def scenario(*args)
  out, status = Open3.capture2(RbConfig.ruby, File.join(__dir__, "scenario.rb"), *args)
  raise "scenario.rb #{args.join(" ")} failed" unless status.success?

  JSON.parse(out.lines.last)
end

def fixture_path(fixture) = File.join(__dir__, "fixtures", fixture)

def generate(fixtures)
  system(RbConfig.ruby, File.join(__dir__, "generate.rb"), *fixtures, exception: true)
end

if options[:profile]
  name, fixture, library = ARGS
  raise OptionParser::MissingArgument, "--profile SCENARIO FIXTURE" unless name && fixture

  names([name], SCENARIOS.keys, "scenario")
  library = names([library || "roo-turbo"], LIBRARIES, "library").first
  fixture = fixture_name(fixture)
  generate([fixture])
  result = scenario(library, name, fixture_path(fixture))
  puts "#{library} #{name} #{fixture}: #{format("%.3f s", result["seconds"])}, #{result["peak_rss"] / MB} MB, " \
       "#{result["allocations"]} objects allocated"
  scenario(library, name, fixture_path(fixture), "profile")
  puts "vernier profile: bench/profiles/#{library}-#{name}-#{fixture}.json (open it in https://vernier.prof)"
  exit
end

RUNS = Integer(ARGS.first, exception: false)
RUN_COUNT = RUNS ? "#{RUNS} runs" : "#{MIN_RUNS}–#{MAX_RUNS} runs, stopping after #{BUDGET_SECONDS} s per scenario"
TIMED = names((RUNS ? ARGS.drop(1) : ARGS).then { |list| list.empty? ? LIBRARIES : list }, LIBRARIES, "library")

def selected_rows(options)
  scenarios = options.fetch(:scenarios, SCENARIOS.keys)
  groups = options.fetch(:groups, GROUPS)
  FIXTURES.intersection(options.fetch(:fixtures, FIXTURES)).flat_map do |fixture|
    SCENARIOS.filter_map do |name, s|
      [name, fixture] if scenarios.include?(name) && groups.include?(s.group) && s.fixtures.include?(fixture)
    end
  end
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

def timed?(library, name, fixture)
  return false unless TIMED.include?(library)
  return true unless REFERENCES.include?(library)

  REFERENCE_SCENARIOS.include?(name) && fixture.end_with?(".xlsx")
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

def ratio(numerator, denominator) = numerator && denominator ? numerator / denominator.to_f : nil
def describe_ratio(value) = value ? format("%.1f×", value) : "-"

def flags(faster, vs_calx)
  [("< #{FLAG_FASTER_THAN_ROO}× roo" if faster && faster < FLAG_FASTER_THAN_ROO),
   ("> #{FLAG_SLOWER_THAN_CALX}× calx" if vs_calx && vs_calx > FLAG_SLOWER_THAN_CALX)].compact
end

filtered = options[:all] || options.values_at(:scenarios, :groups, :fixtures).any?
rows = selected_rows(filtered ? options : { groups: ["default"], fixtures: DEFAULT_FIXTURES })
raise OptionParser::InvalidArgument, "no scenario runs on the fixtures selected" if rows.empty?

generate(rows.map(&:last).uniq)
puts "Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}), #{`uname -sr`.strip}, #{cpu}, #{Etc.nprocessors} cores, " \
     "median of #{RUN_COUNT}", ""
puts "| File | Scenario | roo | roo + roo-turbo | Faster | Less memory | calx (reference) | Time vs calx | " \
     "xsv (reference) | Flag |",
     "|---|---|---:|---:|---:|---:|---:|---:|---:|---|"
flagged = 0
rows.each do |name, fixture|
  path = fixture_path(fixture)
  digests = COMPARED.map { |library| scenario(library, name, path, "digest")["digest"] }
  raise "roo-turbo read different values than roo: #{fixture} #{name}" unless digests.uniq.size == 1

  roo, turbo, calx, xsv = LIBRARIES.map { |library| timed(library, name, path) if timed?(library, name, fixture) }
  faster = ratio(roo&.dig(:seconds), turbo&.dig(:seconds))
  vs_calx = ratio(turbo&.dig(:seconds), calx&.dig(:seconds))
  flag = flags(faster, vs_calx)
  flagged += 1 unless flag.empty?
  puts "| #{fixture} | #{SCENARIOS[name].label} | #{describe(roo)} | #{describe(turbo)} | " \
       "#{describe_ratio(faster)} | #{describe_ratio(ratio(roo&.dig(:peak_rss), turbo&.dig(:peak_rss)))} | " \
       "#{describe(calx)} | #{describe_ratio(vs_calx)} | #{describe(xsv)} | #{flag.join(", ")} |"
end
puts "", "#{rows.size} rows, #{flagged} flagged."
