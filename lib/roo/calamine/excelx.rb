# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx. With options, Roo::Base#each reads every value
    # through #cell, which resolves the sheet and looks up an Array key in its
    # cells hash, one cell at a time. These keep roo's header handling (its own
    # private methods) and read the cells hash with less work per cell.
    # Without options, #each reads its rows through a RowIndex.
    module Excelx
      NO_CELLS = [].freeze
      private_constant :NO_CELLS

      def each(options = {}, &)
        return super if !block_given? || !Calamine.enabled?
        return each_without_options(&) if options.empty?

        headers = header_columns(options)
        @header_line.upto(last_row) do |line|
          # The block may set cells, switch sheets, reload, or close; roo sees
          # that from the next #cell call on, and makes none without headers.
          cells = nil
          yield(headers.each_with_object({}) do |(key, column), row|
            cells ||= sheet_for(nil).cells
            row[key] = safe_send(cells[normalize(line, column)], :value)
          end)
        end
      end

      def parse(options = {})
        return super if block_given? || options.empty? || !Calamine.enabled?

        rows = rows_for(header_columns(options))
        options[:headers] == true ? rows : rows.drop(1)
      end

      def set(...)
        @calamine_writes = (@calamine_writes || 0) + 1
        super
      end

      private

      # Roo::Base#each without options: `1.upto(last_row) { |line| yield row(line) }`.
      def each_without_options
        index = RowIndex.new(self)
        1.upto(last_row) { |line| yield index.row(line) }
      end

      # Roo::Base#each, up to its loop over the rows.
      def header_columns(options)
        clean_sheet_if_need(options)
        search_or_set_header(options)
        @headers || (first_column..last_column).to_h { |column| [cell(@header_line, column), column] }
      end

      def rows_for(headers)
        fields = headers.map { |key, column| [key, normalize(0, column).last] }
        slots = fields.map(&:last).uniq.each_with_index.to_h
        by_line = cells_by_line(slots)
        fields = fields.map { |key, column| [key, slots.fetch(column)] }
        @header_line.upto(last_row).map do |line|
          cells = by_line.fetch(line, NO_CELLS)
          fields.each_with_object({}) do |(key, slot), row|
            cell = cells[slot]
            row[key] = (cell.value if cell.respond_to?(:value)) # Roo::Excelx#safe_send, inline
          end
        end
      end

      def cells_by_line(slots)
        by_line = {}
        sheet_for(nil).cells.each do |(line, column), cell|
          slot = slots[column] or next
          (by_line[line] ||= Array.new(slots.size))[slot] = cell
        end
        by_line
      end
    end
  end
end
