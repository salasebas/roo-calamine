# frozen_string_literal: true

require_relative "test_helper"

class LoadingTest < Minitest::Test
  def test_legacy_entrypoint_loads_the_same_plugin_once
    script = <<~RUBY
      ARGV.each { |path| require path }
      abort "legacy namespace differs" unless Roo::Calamine.equal?(Roo::Turbo)
      puts Roo::Turbo.available?
      puts Roo::Excelx.ancestors.count { |ancestor| ancestor == Roo::Turbo::Excelx }
    RUBY
    %w[roo/calamine roo/turbo].permutation.each do |paths|
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
        require "roo/turbo"
      rescue LoadError => error
        puts error.message
      end
    RUBY
    output = IO.popen([RbConfig.ruby, "-I", File.expand_path("../lib", __dir__), "-e", script],
                      err: %i[child out], &:read)

    assert_equal "Replace roo-calamine with roo-turbo in your Gemfile before loading roo/turbo\n", output
  end
end
