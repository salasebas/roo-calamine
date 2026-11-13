# frozen_string_literal: true

require_relative "test_helper"
require "bundler"
require "open3"

class LoadingTest < Minitest::Test
  def test_bundler_loads_roo_from_only_the_oxide_gem
    Dir.mktmpdir("roo-oxide-bundle") do |dir|
      gemfile = File.join(dir, "Gemfile")
      project = File.expand_path("..", __dir__)
      File.write(gemfile, "source \"https://rubygems.org\"\ngem \"roo-oxide\", path: #{project.dump}\n")
      environment = { "BUNDLE_GEMFILE" => gemfile, "BUNDLE_IGNORE_CONFIG" => "1", "BUNDLE_FROZEN" => "false" }
      Bundler.with_unbundled_env do
        output, status = Open3.capture2e(environment, RbConfig.ruby, "-S", "bundle", "lock", "--local")

        assert_predicate status, :success?, output

        script = "Bundler.require; abort unless defined?(Roo::Excelx); puts Roo::Oxide.enabled?"
        output, status = Open3.capture2e(environment, RbConfig.ruby, "-rbundler/setup", "-e", script)

        assert_predicate status, :success?, output
        assert_equal "true\n", output
      end
    end
  end

  def test_legacy_entrypoint_loads_the_same_plugin_once
    script = <<~RUBY
      ARGV.each { |path| require path }
      abort "legacy namespace differs" unless Roo::Calamine.equal?(Roo::Oxide)
      puts Roo::Oxide.available?
      puts Roo::Excelx.ancestors.count { |ancestor| ancestor == Roo::Oxide::Excelx }
    RUBY
    %w[roo/calamine roo/oxide].permutation.each do |paths|
      output = IO.popen([RbConfig.ruby, "-I", File.expand_path("../lib", __dir__), "-e", script, *paths],
                        err: %i[child out], &:read)

      assert_equal "true\n1\n", output, paths.join(", ")
    end
  end

  def test_preexisting_legacy_plugin_is_rejected
    script = <<~RUBY
      module Roo
        module Calamine; end
      end
      begin
        require "roo/oxide"
      rescue LoadError => error
        puts error.message
      end
    RUBY
    output = IO.popen([RbConfig.ruby, "-I", File.expand_path("../lib", __dir__), "-e", script],
                      err: %i[child out], &:read)

    assert_equal "Replace roo-calamine with roo-oxide in your Gemfile before loading roo/oxide\n", output
  end
end
