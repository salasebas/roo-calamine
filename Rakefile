# frozen_string_literal: true

require "bundler/gem_tasks"
require "minitest/test_task"
require "rb_sys/extensiontask"

GEMSPEC = Gem::Specification.load("roo-calamine.gemspec")

RbSys::ExtensionTask.new("roo_calamine", GEMSPEC) do |ext|
  ext.lib_dir = "lib/roo/calamine"
end

Minitest::TestTask.create(:test) do |t|
  t.test_globs = ["test/**/*_test.rb"]
end

task default: %i[compile test]

desc "Run roo 4.0.0's own Excelx tests with and without roo-calamine and compare"
task parity: :compile do
  ruby "test/parity/run.rb"
end
