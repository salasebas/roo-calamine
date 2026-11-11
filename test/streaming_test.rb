# frozen_string_literal: true

require_relative "test_helper"

class StreamingTest < Minitest::Test
  include OracleAssertions

  # Drops the cells whose value is 2 from every row it reads.
  DROP_TWOS = Module.new do
    def each_cell(row) = super { |cell| yield cell unless cell.value == 2 }
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
end
