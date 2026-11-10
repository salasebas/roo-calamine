# frozen_string_literal: true

# Runs roo's own Excelx tests from the roo 4.0.0 tag twice, once with plain
# roo and once with roo-calamine loaded, and compares the results. roo is the
# oracle: the plugin run must fail exactly the tests the plain run fails.
#
#   bundle exec rake parity

require "fileutils"
require "json"
require "open3"
require "rbconfig"

ROO_TAG = "v4.0.0"
ROO_SHA = "a25a8cf97e048ab1e152e5d5a70701eaa518a6bf"
ROOT = File.expand_path("../..", __dir__)
CHECKOUT = File.join(ROOT, "tmp", "roo-#{ROO_TAG}")
PROBE = File.join(__dir__, "probe.rb")

SUITES = {
  "test/roo/test_excelx.rb" => :minitest,
  "test/test_roo.rb" => :minitest,
  "test/excelx" => :minitest,
  "spec/lib/roo/excelx_spec.rb" => :rspec,
  "spec/lib/roo/strict_spec.rb" => :rspec,
  "spec/lib/roo/excelx" => :rspec
}.freeze

def checkout_roo
  unless File.directory?(CHECKOUT)
    FileUtils.mkdir_p(File.dirname(CHECKOUT))
    system("git", "clone", "--quiet", "--depth", "1", "--branch", ROO_TAG,
           "https://github.com/roo-rb/roo.git", CHECKOUT, exception: true)
  end
  sha = IO.popen(["git", "-C", CHECKOUT, "rev-parse", "HEAD"], &:read).strip
  abort "#{CHECKOUT} is at #{sha}, expected #{ROO_SHA}" unless sha == ROO_SHA
end

def command(kind, path, probe_out)
  ruby = [RbConfig.ruby, "-I", File.join(ROOT, "lib"), "-r", PROBE]
  case kind
  when :minitest
    files = File.directory?(File.join(CHECKOUT, path)) ? Dir.glob("#{path}/**/test_*.rb", base: CHECKOUT) : [path]
    [*ruby, "-I", "test", "-e", "ARGV.each { |f| require File.expand_path(f) }", *files.sort]
  when :rspec
    [*ruby, "-S", "rspec", "--no-color", path]
  end.then { |argv| [{ "PARITY_PROBE_OUT" => probe_out }, *argv] }
end

def parse(kind, output)
  case kind
  when :minitest
    summary = output[/(\d+) runs, \d+ assertions, (\d+) failures, (\d+) errors, (\d+) skips/] or return
    runs, failures, errors, skips = Regexp.last_match.captures.map(&:to_i)
    failed = output.scan(/^\s*\d+\) (?:Failure|Error):\n(\S+?)(?: \[|:|\n)/).flatten
    { summary:, total: runs, failed: failures + errors, skipped: skips, failures: failed.sort }
  when :rspec
    summary = output[/(\d+) examples?, (\d+) failures?(?:, (\d+) pending)?/] or return
    total, failed, pending = Regexp.last_match.captures.map(&:to_i)
    { summary:, total:, failed:, skipped: pending, failures: output.scan(/^rspec (\S+)/).flatten.sort }
  end
end

def run_suite(path, kind, mode)
  probe_out = File.join(ROOT, "tmp", "parity-probe-#{mode}.json")
  FileUtils.rm_f(probe_out)
  env, *argv = command(kind, path, probe_out)
  output, = Open3.capture2e(env.merge("PARITY_MODE" => mode.to_s), *argv, chdir: CHECKOUT)
  result = parse(kind, output) or abort "could not read the #{kind} result of #{path} (#{mode}):\n#{output}"
  result.merge(probe: File.exist?(probe_out) ? JSON.parse(File.read(probe_out)) : {})
end

checkout_roo
rows = []
mismatch = false
SUITES.each do |path, kind|
  roo = run_suite(path, kind, :roo)
  plugin = run_suite(path, kind, :plugin)
  same = roo.slice(:total, :failed, :skipped, :failures) == plugin.slice(:total, :failed, :skipped, :failures)
  mismatch ||= !same
  rows << [path, kind, roo, plugin, same]
  warn "#{path}: roo failed #{roo[:failures].inspect}, roo-calamine failed #{plugin[:failures].inspect}" unless same
end

HEADER = ["Suite", "Tests", "roo: failed / skipped", "roo-calamine: failed / skipped",
          "Native readers", "roo cell parses (plugin run)", "Same"].freeze
puts "| #{HEADER.join(" | ")} |", "|---|---:|---:|---:|---:|---:|---|"
rows.each do |path, _kind, roo, plugin, same|
  cells = ["`#{path}`", roo[:total], "#{roo[:failed]} / #{roo[:skipped]}", "#{plugin[:failed]} / #{plugin[:skipped]}",
           plugin[:probe].fetch("native_readers", 0), plugin[:probe].fetch("roo_cells", 0), same ? "yes" : "NO"]
  puts "| #{cells.join(" | ")} |"
end
abort "roo-calamine results differ from roo" if mismatch
