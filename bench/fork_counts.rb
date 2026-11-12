# frozen_string_literal: true

require "json"
require "open3"
require "rbconfig"
require "tempfile"

upstream, base = ARGV
abort "usage: ruby bench/fork_counts.rb UPSTREAM_CHECKOUT [BASELINE]" unless upstream
base ||= "194b149d64a2682fa43fc54319a640dfe4eb300e"
files, status = Open3.capture2("git", "ls-tree", "-r", "--name-only", base, "lib", "ext/roo_calamine/src")
abort "cannot list baseline" unless status.success?
production = ->(text) { text.split(/^#\[cfg\(test\)\]/, 2).first.to_s.lines.size }
before_files = files.lines.map(&:strip).grep(/\.(rs|rb)\z/).reject { |path| path.end_with?("/tests.rs") }
before = before_files.sum do |path|
  text, read = Open3.capture2("git", "show", "#{base}:#{path}")
  abort "cannot read #{path}" unless read.success?
  production.call(text)
end
after_files = Dir["lib/**/*.rb", "ext/roo_calamine/src/**/*.rs"].reject { |path| path.end_with?("/tests.rs") }
after = after_files.sum { |path| production.call(File.read(path)) }
patch, generated = Open3.capture2(RbConfig.ruby, File.expand_path("../script/calamine_patch.rb", __dir__), upstream)
abort "cannot generate fork patch" unless generated.success?
stats, counted = Open3.capture2("git", "apply", "--numstat", stdin_data: patch)
abort "cannot count fork patch" unless counted.success?
fork = stats.lines.filter_map do |line|
  path = line.strip.split("\t").last
  next unless path.start_with?("src/") && !path.end_with?("/tests.rs")

  original = File.join(upstream, path)
  candidate = File.expand_path("../vendor/calamine/#{path}", __dir__)
  added, deleted = Tempfile.create("fork-before") do |old|
    Tempfile.create("fork-after") do |replacement|
      old.write(File.file?(original) ? File.read(original).split(/^#\[cfg\(test\)\]/, 2).first : "")
      replacement.write(File.read(candidate).split(/^#\[cfg\(test\)\]/, 2).first)
      [old, replacement].each(&:flush)
      diff, compared = Open3.capture2("git", "diff", "--no-index", "--numstat", old.path, replacement.path)
      abort "cannot count #{path}" unless compared.exitstatus <= 1
      diff.empty? ? [0, 0] : diff.split("\t", 3).first(2).map { |count| Integer(count) }
    end
  end
  [path, { added:, deleted:, new: !File.file?(original) }]
end.to_h
fork_new = fork.values.select { |entry| entry[:new] }.sum { |entry| entry[:added] }
fork_upstream_added = fork.values.reject { |entry| entry[:new] }.sum { |entry| entry[:added] }
fork_upstream_deleted = fork.values.sum { |entry| entry[:deleted] }
owned_after = after + fork_new + fork_upstream_added
puts JSON.pretty_generate(baseline: base, before:, remaining: after, fork_new:, fork_upstream_added:,
                          fork_upstream_deleted:, owned_after:, net_saved: before - owned_after, fork:)
