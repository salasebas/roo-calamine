# frozen_string_literal: true

require_relative "test_helper"

# Every .xlsx and .xlsm fixture in roo 4.0.0's test suite, read through each
# public path, must come out exactly as roo reads it.
class RooFixturesTest < Minitest::Test
  include OracleAssertions

  FILES = File.join(RooCheckout.ensure!, "test", "files")

  # Worksheets the native reader hands to roo on purpose.
  ROO_READS = {
    "Pfand_from_windows_phone.xlsx" => "worksheets are UTF-16",
    "Pfand_from_windows_phone.xlsm" => "worksheets are UTF-16"
  }.freeze

  OPEN_OPTIONS = [{}, { expand_merged_ranges: true }, { empty_cell: true },
                  { disable_html_wrapper: true }, { no_hyperlinks: true }].freeze

  Dir.glob("*.{xlsx,xlsm}", base: FILES).sort.each do |name|
    path = File.join(FILES, name)
    native_only = !ROO_READS.key?(name)
    slug = name.gsub(/\W+/, "_")

    define_method("test_cells_#{slug}") do
      OPEN_OPTIONS.each do |options|
        assert_same_as_roo("#{name} cells #{options}", native_only:) do
          Oracle.describe_cells(Roo::Excelx.new(path, options.dup))
        end
      end
    end

    define_method("test_each_row_streaming_#{slug}") do
      Oracle::STREAM_OPTIONS.each do |options|
        assert_same_as_roo("#{name} each_row_streaming #{options}", native_only:) do
          Oracle.describe_stream(Roo::Excelx.new(path), options)
        end
      end
    end

    define_method("test_each_row_values_#{slug}") do
      OPEN_OPTIONS.each do |options|
        assert_values_same_as_roo("#{name} each_row_values #{options}", path, options, native_only:)
      end
    end

    define_method("test_roo_api_#{slug}") do
      assert_same_as_roo("#{name} API", native_only:) do
        Oracle.describe_api(Roo::Spreadsheet.open(path))
      end
    end

    define_method("test_lookups_#{slug}") do
      OPEN_OPTIONS.each do |options|
        lazy = native_only && !options[:expand_merged_ranges]
        assert_same_as_roo("#{name} lookups #{options}", native_only:, lazy:) do
          Oracle.describe_lookups(Roo::Excelx.new(path, options.dup))
        end
      end
    end
  end
end
