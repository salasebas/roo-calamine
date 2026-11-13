# frozen_string_literal: true

require_relative "lib/roo/oxide/version"

Gem::Specification.new do |spec|
  spec.name = "roo-oxide"
  spec.version = Roo::Oxide::VERSION
  spec.authors = ["Sebastian Sala"]

  spec.summary = "Rust-powered native acceleration for Roo's XLSX and XLSM readers."
  spec.description = <<~DESCRIPTION.strip.gsub(/\s+/, " ")
    An unofficial acceleration layer built on top of Roo. Add roo-oxide to the
    Gemfile; Roo is installed as a dependency and its spreadsheet API stays the
    same. A Rust extension speeds up XLSX and XLSM reads while preserving Roo's
    cell and value rules. Unsupported input uses Roo's own reader. Not affiliated
    with roo-rb.
  DESCRIPTION
  spec.homepage = "https://github.com/salasebas/roo-oxide"
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
    "ext/roo_oxide/{Cargo.toml,extconf.rb}",
    "ext/roo_oxide/src/**/*.rs",
    "Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "CHANGELOG.md"
  ]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/roo_oxide/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9.130"
  spec.add_dependency "roo", "~> 4.0"
end
