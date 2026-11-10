# frozen_string_literal: true

require "minitest/autorun"
require "roo/calamine"
require_relative "support/oracle"
require_relative "support/roo_checkout"
require_relative "support/workbook"

module OracleAssertions
  # Asserts that roo-calamine returns what roo returns for `description`, and
  # that the native reader, not roo's parser, produced it.
  def assert_same_as_roo(label, native_only: true, &)
    roo, = Oracle.read(native: false, &)
    native, counts = Oracle.read(native: true, &)

    assert_equal roo, native, "#{label}: roo-calamine differs from roo"
    return unless native_only && roo.first == :ok

    assert_operator counts[:native_readers], :>, 0, "#{label}: the native reader did not run"
    assert_equal 0, counts[:roo_cells], "#{label}: roo parsed cells itself"
  end

  # Asserts that Roo::Calamine.each_row_values, with the native reader on and
  # off, yields the values of the cells roo's own
  # each_row_streaming(pad_cells: true) yields.
  def assert_values_same_as_roo(label, path, options = {}, native_only: true)
    roo, = Oracle.read(native: false) { Oracle.describe_streamed_values(Roo::Excelx.new(path, options.dup)) }
    fallback, = Oracle.read(native: false) { Oracle.describe_each_row_values(path, options) }
    native, counts = Oracle.read(native: true) { Oracle.describe_each_row_values(path, options) }

    assert_equal roo, fallback, "#{label}: each_row_values without the native reader differs from roo"
    assert_equal roo, native, "#{label}: each_row_values differs from roo"
    return unless native_only && roo.first == :ok

    assert_operator counts[:native_readers], :>, 0, "#{label}: the native reader did not run"
    assert_equal 0, counts[:roo_cells], "#{label}: roo parsed cells itself"
  end
end
