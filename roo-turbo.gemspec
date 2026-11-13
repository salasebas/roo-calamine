# frozen_string_literal: true

require_relative "lib/roo/turbo/version"

Gem::Specification.new do |spec|
  spec.name = "roo-turbo"
  spec.version = Roo::Turbo::VERSION
  spec.authors = ["Sebastian Sala"]

  spec.summary = "A native reader that makes roo read .xlsx and .xlsm files faster, with the same results."
  spec.description = <<~DESCRIPTION.strip.gsub(/\s+/, " ")
    Unofficial plugin for roo. Add it to the Gemfile and Roo::Excelx reads worksheets
    through a Rust extension instead of Nokogiri, building the same Roo::Excelx::Cell
    objects. Anything the native reader does not reproduce exactly goes to roo's own
    code. Not affiliated with roo-rb.
  DESCRIPTION
  spec.homepage = "https://github.com/salasebas/roo-calamine"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.3"
  spec.required_rubygems_version = ">= 3.3.11"

  spec.metadata["homepage_uri"] = spec.homepage
  spec.metadata["source_code_uri"] = "#{spec.homepage}/tree/v#{spec.version}"
  spec.metadata["changelog_uri"] = "#{spec.homepage}/blob/v#{spec.version}/CHANGELOG.md"
  spec.metadata["bug_tracker_uri"] = "#{spec.homepage}/issues"
  spec.metadata["rubygems_mfa_required"] = "true"

  spec.files = Dir[
    "lib/**/*.rb",
    "ext/roo_turbo/{Cargo.toml,extconf.rb}",
    "ext/roo_turbo/src/**/*.rs",
    "Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "CHANGELOG.md"
  ]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/roo_turbo/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9.130"
  spec.add_dependency "roo", "~> 4.0"
end
