# frozen_string_literal: true

# Checks a built gem before it can be published:
#
#   ruby script/check_gem.rb GEM PLATFORM [RUBY_VERSIONS]
#
# PLATFORM is `ruby` for the source gem. For a platform gem, RUBY_VERSIONS is
# the comma-separated list of Ruby minor versions it must carry a binary for.
require "rubygems/package"

path, platform, ruby_versions = ARGV
abort "usage: check_gem.rb GEM PLATFORM [RUBY_VERSIONS]" unless path && platform

spec = Gem::Package.new(path).spec
files = spec.files
errors = []
errors << "name is #{spec.name}" unless spec.name == "roo-oxide"
errors << "platform is #{spec.platform}, expected #{platform}" unless spec.platform.to_s.start_with?(platform)
%w[lib/roo/oxide.rb lib/roo/calamine.rb lib/roo/calamine/version.rb
   lib/roo/oxide/sheet_doc.rb lib/roo/oxide/context.rb LICENSE README.md].each do |file|
  errors << "#{file} is missing" unless files.include?(file)
end
errors << "requires roo #{spec.dependencies.find { |d| d.name == "roo" }&.requirement}" unless
  spec.dependencies.any? { |d| d.name == "roo" && d.requirement.to_s == "~> 4.0" }

if platform == "ruby"
  errors << "extensions are #{spec.extensions}" unless spec.extensions == ["ext/roo_oxide/extconf.rb"]
  %w[ext/roo_oxide/src/lib.rs Cargo.toml Cargo.lock].each do |f|
    errors << "#{f} is missing" unless files.include?(f)
  end
  errors << "carries a compiled binary" if files.grep(/\.(so|bundle|dll)\z/).any?
else
  errors << "a platform gem must not compile on install" unless spec.extensions.empty?
  ruby_versions.split(",").each do |version|
    binary = files.grep(%r{\Alib/roo/oxide/#{Regexp.escape(version)}/roo_oxide\.(so|bundle|dll)\z})
    errors << "no binary for Ruby #{version}" if binary.empty?
  end
end

abort("#{File.basename(path)}:\n  #{errors.join("\n  ")}") unless errors.empty?
puts "#{File.basename(path)}: #{spec.platform}, #{files.size} files, required Ruby #{spec.required_ruby_version}"
