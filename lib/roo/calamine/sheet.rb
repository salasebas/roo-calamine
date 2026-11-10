# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::Sheet: single-cell lookups, rows, columns, and
    # the sheet's bounds read the lazy cells of SheetDoc#lazy_cells while
    # #cells has not been built, so they build only the cells they read.
    module Sheet
      # cells[key]
      def cell_at(key)
        (lazy = lazy_cells) ? lazy[key] : cells[key]
      end

      def row(row_number)
        first_column.upto(last_column).map do |col|
          cell_at([row_number, col])&.value
        end
      end

      def column(col_number)
        first_row.upto(last_row).map do |row|
          cell_at([row, col_number])&.value
        end
      end

      def excelx_format(key)
        cell = cell_at(key)
        styles.style_format(cell.style).to_s if cell
      end

      private

      def lazy_cells
        @sheet.lazy_cells(@rels) unless @cells
      end

      def first_last_row_col
        @first_last_row_col ||= lazy_cells&.bounds || super
      end
    end
  end
end
