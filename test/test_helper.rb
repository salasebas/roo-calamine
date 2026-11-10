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
end
