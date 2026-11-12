# frozen_string_literal: true

require "json"
require "open3"

base = ARGV.fetch(0, "21f9da66f1260eb41469f1b6ef57a98b8d57f871")
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
fork = Dir["ext/roo_calamine/calamine/src/roo/**/*.rs"].reject { |path| path.end_with?("/tests.rs") }
relocated = fork.sum { |path| production.call(File.read(path)) }
upstream_edits = 2
puts JSON.pretty_generate(baseline: base, before:, remaining: after, fork_new: relocated,
                          fork_upstream_edits: upstream_edits, owned_after: after + relocated + upstream_edits,
                          net_saved: before - after - relocated - upstream_edits)
