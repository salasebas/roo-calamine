# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::Sheet: single-cell lookups, rows, columns, and
    # the sheet's bounds read the lazy cells of SheetDoc#lazy_cells while
    # #cells has not been built, so they build only the cells they read. Rows
    # and columns build only the cells whose value needs one.
    module Sheet
      NO_CELLS = [].freeze
      private_constant :NO_CELLS

      # cells[[row, column]]
      def cell_at(row, column)
        (lazy = lazy_cells) ? lazy.cell_at(row, column) : cells[[row, column]]
      end

      def row(row_number)
        lazy = lazy_cells
        return lazy.row(row_number, first_column, last_column) if lazy && first_column

        first_column.upto(last_column).map do |col|
          cell_at(row_number, col)&.value
        end
      end

      def column(col_number)
        lazy = lazy_cells
        return lazy.column(col_number, first_row, last_row) if lazy && first_row

        first_row.upto(last_row).map do |row|
          cell_at(row, col_number)&.value
        end
      end

      def excelx_format(key)
        cell = cell_at(*key)
        styles.style_format(cell.style).to_s if cell
      end

      # For Excelx#parse: a Hash per line of `lines`, holding the value of the
      # cell at [line, column] under `key` for each [key, column] of `fields`.
      def parse_rows(lines, fields)
        lazy = lazy_cells
        if lazy && fields.all? { |_, column| column.is_a?(Integer) }
          lazy_parse_rows(lazy, lines, fields)
        else
          cells_parse_rows(lines, fields)
        end
      end

      # The Native::Store behind the lazy cells while #cells is not built.
      def lazy_cells
        @sheet.lazy_cells(@rels) unless @cells
      end

      private

      def lazy_parse_rows(lazy, lines, fields)
        first, last = fields.map(&:last).minmax
        slots = fields.map { |key, column| [key, column - first] }.freeze
        lines.map { |line| lazy.hash_row(line, first || 1, last || 0, slots) }
      end

      def cells_parse_rows(lines, fields)
        slots = fields.map(&:last).uniq.each_with_index.to_h
        by_line = cells_by_line(slots)
        fields = fields.map { |key, column| [key, slots.fetch(column)] }
        lines.map do |line|
          line_cells = by_line.fetch(line, NO_CELLS)
          fields.each_with_object({}) do |(key, slot), row|
            cell = line_cells[slot]
            row[key] = (cell.value if cell.respond_to?(:value)) # Roo::Excelx#safe_send, inline
          end
        end
      end

      def cells_by_line(slots)
        by_line = {}
        cells.each do |(line, column), cell|
          slot = slots[column] or next
          (by_line[line] ||= Array.new(slots.size))[slot] = cell
        end
        by_line
      end

      # roo's loop over #each_cell only copies the cells when it pads none.
      def cells_for_row_element(row_element, options = {})
        (@sheet.native_row_cells(row_element) unless options[:pad_cells]) || super
      end

      def first_last_row_col
        @first_last_row_col ||= lazy_cells&.bounds || super
      end
    end
  end
end
