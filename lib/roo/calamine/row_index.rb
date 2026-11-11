# frozen_string_literal: true

module Roo
  module Calamine
    # Answers Excelx#row for the default sheet from its cells in
    # first_row..last_row and first_column..last_column, laid out in one pass
    # over Sheet#cells: in one flat Array with a slot per column of each row
    # (an Array per row costs a major GC on a large sheet), or, on a sheet
    # with fewer than one cell per MAX_SLOTS_PER_CELL slots, in a Hash of an
    # Array per row of its column offsets and cells. roo answers the first ROO_ROWS
    # requests, so a caller that stops early never pays for that pass, and
    # any other row, a sheet too short to repay the pass (fewer than
    # 3 * ROO_ROWS rows), an empty sheet, and every request once the default
    # sheet changes or Excelx#set writes to the workbook.
    class RowIndex
      ROO_ROWS = 1_000
      MAX_SLOTS_PER_CELL = 4

      def initialize(workbook)
        @workbook = workbook
        @requests = 0
      end

      def row(line, set_calls)
        index = index(line) if current?(set_calls)
        return @workbook.row(line) unless index

        @lines ? sparse_row(@lines[index]) : @cells[index * @width, @width].map! { |cell| cell&.value }
      end

      private

      def index(line)
        line - @first_row if line.is_a?(Integer) && line.between?(@first_row, @last_row)
      end

      def current?(set_calls)
        return false if (@requests += 1) <= ROO_ROWS

        sheet = @workbook.sheet_for(nil)
        build(sheet, set_calls) unless @sheet
        (@cells || @lines) && @sheet.equal?(sheet) && @set_calls == set_calls
      end

      def sparse_row(entries)
        values = Array.new(@width)
        return values unless entries

        i = 0
        while i < entries.size
          values[entries[i]] = entries[i + 1]&.value
          i += 2
        end
        values
      end

      def build(sheet, set_calls)
        @sheet = sheet
        @set_calls = set_calls
        return unless sheet.first_row && sheet.last_row >= 3 * ROO_ROWS

        @first_row = sheet.first_row
        @last_row = sheet.last_row
        @first = sheet.first_column
        @last = sheet.last_column
        @width = @last - @first + 1
        lay_out(sheet.cells, @last_row - @first_row + 1)
      end

      def lay_out(cells, rows)
        if rows * @width > MAX_SLOTS_PER_CELL * cells.size
          @lines = {}
          each_cell(cells) { |index, column, cell| (@lines[index] ||= []).push(column, cell) }
        else
          @cells = Array.new(rows * @width)
          each_cell(cells) { |index, column, cell| @cells[(index * @width) + column] = cell }
        end
      end

      def each_cell(cells) # rubocop:disable Metrics/CyclomaticComplexity, Metrics/PerceivedComplexity
        cells.each_pair do |key, cell|
          next unless key.is_a?(Array) && key.size == 2

          line, column = key
          next unless line.is_a?(Integer) && column.is_a?(Integer) &&
                      line >= @first_row && line <= @last_row && column >= @first && column <= @last

          yield line - @first_row, column - @first, cell
        end
      end
    end
  end
end
