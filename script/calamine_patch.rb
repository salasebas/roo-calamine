# frozen_string_literal: true

require "open3"

upstream = ARGV.fetch(0) { abort "usage: ruby script/calamine_patch.rb UPSTREAM_CHECKOUT > roo.patch" }
upstream = File.expand_path(upstream)
vendor = File.expand_path("../ext/roo_calamine/calamine", __dir__)
revision = "a4dba3b5309d81b2774018a13be39e68d938c3c7"
head, status = Open3.capture2("git", "-C", upstream, "rev-parse", "HEAD")
abort "expected Calamine #{revision}, got #{head.strip}" unless status.success? && head.strip == revision
dirty, checked = Open3.capture2("git", "-C", upstream, "status", "--porcelain")
abort "use a clean upstream checkout" unless checked.success? && dirty.empty?
files = Dir.glob("{Cargo.toml,LICENSE-MIT.md,README.md,Changelog.md,src/**/*.rs,examples/**/*.rs,benches/**/*.rs}",
                 base: vendor).sort
upstream_edits = %w[src/lib.rs src/utils.rs].freeze
files.each do |path|
  original = File.join(upstream, path)
  replacement = File.join(vendor, path)
  next if File.file?(original) && File.binread(original) == File.binread(replacement)

  abort "unexpected upstream edit: #{path}" unless path.start_with?("src/roo/") ||
                                                   upstream_edits.include?(path)
  original = File::NULL unless File.file?(original)
  patch, result = Open3.capture2("git", "diff", "--no-index", "--", original, replacement)
  abort "git diff failed for #{path}" unless result.exitstatus <= 1
  patch = patch.gsub("a#{original}", "a/#{path}").gsub("b#{replacement}", "b/#{path}")
  patch = patch.gsub("a#{replacement}", "a/#{path}") if original == File::NULL
  print patch
end
