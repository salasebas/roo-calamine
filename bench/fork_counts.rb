# frozen_string_literal: true

require "json"
require "open3"

base = ARGV.fetch(0, "43914e90cc018ba07fcc29f92d3e2a866ab3eccb")
files, status = Open3.capture2("git", "ls-tree", "-r", "--name-only", base, "lib", "ext/roo_calamine/src")
abort "cannot list baseline" unless status.success?
production = ->(text) { text.split(/^#\[cfg\(test\)\]/, 2).first.to_s.lines.size }
before = 0
after = 0
files.lines.map(&:strip).grep(/\.(rs|rb)\z/).reject { |path| path.end_with?("/tests.rs") }.each do |path|
  text, read = Open3.capture2("git", "show", "#{base}:#{path}")
  abort "cannot read #{path}" unless read.success?
  before += production.call(text)
  after += production.call(File.read(path)) if File.file?(path)
end
fork = Dir["ext/roo_calamine/calamine/src/roo/**/*.rs"].reject { |path| path.end_with?("/tests.rs") }
relocated = fork.sum { |path| production.call(File.read(path)) }
upstream_edits = 2
puts JSON.pretty_generate(baseline: base, before:, remaining: after, fork_new: relocated,
                          fork_upstream_edits: upstream_edits, owned_after: after + relocated + upstream_edits,
                          net_saved: before - after - relocated - upstream_edits)
