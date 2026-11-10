# frozen_string_literal: true

module Roo
  module Calamine
    # Answers Excelx#row for the default sheet from its cells in
    # first_row..last_row and first_column..last_column, laid out in one flat
    # Array (an Array per row costs a major GC on a large sheet) in one pass
    # over Sheet#cells. roo answers the first ROO_ROWS requests, so a caller
    # that stops early never pays for that pass, and any other row, a sheet
    # too short to repay the pass (fewer than 3 * ROO_ROWS rows), an empty or
    # too sparse sheet, and every request once the default sheet changes or
    # Excelx#set writes to the workbook.
    class RowIndex
      ROO_ROWS = 1_000
      MAX_SLOTS_PER_CELL = 4

      def initialize(workbook)
        @workbook = workbook
        @requests = 0
      end

      # `writes` is how many times Excelx#set has run on the workbook.
      def row(line, writes)
        offset = offset(line) if current?(writes)
        return @workbook.row(line) unless offset

        @cells[offset, @width].map! { |cell| cell&.value }
      end

      private

      def offset(line)
        (line - @first_row) * @width if line.is_a?(Integer) && line.between?(@first_row, @last_row)
      end

      def current?(writes)
        return false if (@requests += 1) <= ROO_ROWS

        sheet = @workbook.sheet_for(nil)
        build(sheet, writes) unless @sheet
        @cells && @sheet.equal?(sheet) && @writes == writes
      end

      def build(sheet, writes)
        @sheet = sheet
        @writes = writes
        return unless sheet.first_row && sheet.last_row >= 3 * ROO_ROWS

        @first_row = sheet.first_row
        @last_row = sheet.last_row
        @first = sheet.first_column
        @last = sheet.last_column
        @width = @last - @first + 1
        size = (@last_row - @first_row + 1) * @width
        lay_out(sheet.cells, size) unless size > MAX_SLOTS_PER_CELL * sheet.cells.size
      end

      def lay_out(cells, size) # rubocop:disable Metrics/CyclomaticComplexity, Metrics/PerceivedComplexity
        @cells = Array.new(size)
        cells.each_pair do |key, cell|
          next unless key.is_a?(Array) && key.size == 2

          line, column = key
          next unless line.is_a?(Integer) && column.is_a?(Integer) &&
                      line >= @first_row && line <= @last_row && column >= @first && column <= @last

          @cells[((line - @first_row) * @width) + column - @first] = cell
        end
      end
    end
  end
end
