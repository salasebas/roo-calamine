# frozen_string_literal: true

require "bundler/gem_tasks"
require "minitest/test_task"
require "rb_sys/extensiontask"

GEMSPEC = Gem::Specification.load("roo-turbo.gemspec")

RbSys::ExtensionTask.new("roo_turbo", GEMSPEC) do |ext|
  ext.lib_dir = "lib/roo/turbo"
end

Minitest::TestTask.create(:test) do |t|
  t.test_globs = ["test/**/*_test.rb"]
end

task default: %i[compile test]

desc "Run roo 4.0.0's own Excelx tests with and without roo-turbo and compare"
task parity: :compile do
  ruby "test/parity/run.rb"
end
