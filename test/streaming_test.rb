# frozen_string_literal: true

require_relative "test_helper"

class StreamingTest < Minitest::Test
  include OracleAssertions

  # Drops the cells whose value is 2 from every row it reads.
  DROP_TWOS = Module.new do
    def each_cell(row) = super { |cell| yield cell unless cell.value == 2 }
  end

  EXTRA_PADDING = Module.new do
    def pad_cells(cell, last_column) = super + [nil]
  end

  def setup
    @dir = Dir.mktmpdir("roo-calamine")
  end

  def teardown
    FileUtils.rm_rf(@dir)
  end

  def test_streaming_rows_go_through_a_custom_each_cell
    rows = (1..3).map { |i| %(<row r="#{i}"><c r="A#{i}"><v>1</v></c><c r="B#{i}"><v>2</v></c><c r="D#{i}"><v>3</v></c></row>) }
    path = Workbook.write(@dir, "book", sheets: { "Sheet1" => Workbook.sheet(rows.join) })

    Oracle::STREAM_OPTIONS.each do |options|
      assert_same_as_roo("custom each_cell #{options}") do
        xlsx = Roo::Excelx.new(path)
        xlsx.sheet_for(nil).instance_variable_get(:@sheet).singleton_class.prepend(DROP_TWOS)
        Oracle.describe_stream(xlsx, options)
      end
    end
  end

  def test_streaming_rows_keep_custom_padding
    sheet = Workbook.sheet(%(<row r="1"><c r="A1"><v>1</v></c><c r="C1"><v>3</v></c></row>))
    path = Workbook.write(@dir, "padding", sheets: { "Sheet1" => sheet })
    read = lambda do
      xlsx = Roo::Excelx.new(path)
      xlsx.sheet_for(nil).singleton_class.prepend(EXTRA_PADDING)
      Oracle.describe_stream(xlsx, pad_cells: true)
    end

    assert_equal :ok, Oracle.read(native: false, &read).first.first
    assert_same_as_roo("custom padding", &read)
  end
end
