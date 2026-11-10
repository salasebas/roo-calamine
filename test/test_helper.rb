# frozen_string_literal: true

require "minitest/autorun"
require "roo/calamine"
require_relative "support/oracle"
require_relative "support/roo_checkout"
require_relative "support/workbook"

module OracleAssertions
  # Asserts that roo-calamine returns what roo returns for `description`, and
  # that the native reader, not roo's parser, produced it; with `lazy`, that
  # it built cells one at a time as they were read.
  def assert_same_as_roo(label, native_only: true, lazy: false, &)
    roo, = Oracle.read(native: false, &)
    native, counts = Oracle.read(native: true, &)

    assert_equal roo, native, "#{label}: roo-calamine differs from roo"
    return unless native_only && roo.first == :ok

    assert_operator counts[:native_readers], :>, 0, "#{label}: the native reader did not run"
    assert_equal 0, counts[:roo_cells], "#{label}: roo parsed cells itself"
    assert_operator counts[:lazy_stores], :>, 0, "#{label}: cells were built all at once" if lazy
  end

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

# On sheets of 3 rows or more, roo answers the first row of each #each call
# and the row index the rest, so small sheets cover both and the hand-over.
Roo::Calamine::RowIndex.send(:remove_const, :ROO_ROWS)
Roo::Calamine::RowIndex.const_set(:ROO_ROWS, 1)
