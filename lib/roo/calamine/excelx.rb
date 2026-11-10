# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx. With options, Roo::Base#each reads every value
    # through #cell, which resolves the sheet and looks up an Array key in its
    # cells hash, one cell at a time. These keep roo's header handling (its own
    # private methods) and read the cells hash with less work per cell.
    #
    # roo's single-cell lookups read Sheet#cell_at instead of Sheet#cells, so
    # they build only the cells they read. #each and #parse read every cell, so
    # they build them all at once, as roo does.
    module Excelx
      NO_CELLS = [].freeze
      private_constant :NO_CELLS

      def each(options = {})
        return super unless block_given?

        if options.empty?
          build_cells
          return super
        end
        return super unless Calamine.enabled?

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

      def cell(row, col, sheet = nil)
        safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :value)
      end

      def formula(row, col, sheet = nil)
        safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :formula)
      end

      def font(row, col, sheet = nil)
        definition_index = safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :style)
        styles.definitions[definition_index] if definition_index
      end

      def celltype(row, col, sheet = nil)
        safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :type)
      end

      def excelx_type(row, col, sheet = nil)
        safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :cell_type)
      end

      def excelx_value(row, col, sheet = nil)
        safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :cell_value)
      end

      def formatted_value(row, col, sheet = nil)
        safe_send(sheet_for(sheet).cell_at(normalize(row, col)), :formatted_value)
      end

      def empty?(row, col, sheet = nil)
        sheet = sheet_for(sheet)
        cell = sheet.cell_at(normalize(row, col))
        !cell || cell.empty? ||
          (row < sheet.first_row || row > sheet.last_row || col < sheet.first_column || col > sheet.last_column)
      end

      private

      # Roo::Base#each, up to its loop over the rows.
      def header_columns(options)
        clean_sheet_if_need(options)
        build_cells
        search_or_set_header(options)
        @headers || (first_column..last_column).to_h { |column| [cell(@header_line, column), column] }
      end

      # Builds every cell before roo's next step, which reads them all. When
      # that raises, roo's own read raises the same error where roo raises it.
      def build_cells
        sheet_for(nil).cells
      rescue StandardError
        nil
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
