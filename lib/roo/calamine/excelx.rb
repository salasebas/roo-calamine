# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx. With options, Roo::Base#each reads every value
    # through #cell, which resolves the sheet and looks up an Array key in its
    # cells hash, one cell at a time. These keep roo's header handling (its own
    # private methods) and read the cells with less work per cell.
    #
    # roo's single-cell lookups read Sheet#cell_at instead of Sheet#cells, and
    # rows read Sheet#row, so they build only the cells their values need.
    # #each without options reads the native store sequentially, or uses a
    # RowIndex once every cell is built.
    module Excelx
      def each(options = {}, &)
        return super unless block_given?

        if options.empty?
          build_cells
          return native_rows? ? each_without_options(&) : super
        end
        return super unless Calamine.enabled?

        each_with_headers(header_columns(options), &)
      end

      def parse(options = {})
        return super if block_given? || options.empty? || !Calamine.enabled?

        rows = rows_for(header_columns(options))
        options[:headers] == true ? rows : rows.drop(1)
      end

      def cell(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        cell.value if cell.respond_to?(:value)
      end

      def formula(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        cell.formula if cell.respond_to?(:formula)
      end

      def font(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        definition_index = cell.style if cell.respond_to?(:style)
        styles.definitions[definition_index] if definition_index
      end

      def celltype(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        cell.type if cell.respond_to?(:type)
      end

      def excelx_type(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        cell.cell_type if cell.respond_to?(:cell_type)
      end

      def excelx_value(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        cell.cell_value if cell.respond_to?(:cell_value)
      end

      def formatted_value(row, col, sheet = nil)
        row, col = normalize(row, col) if row.is_a?(String) || col.is_a?(String)
        cell = sheet_for(sheet).cell_at(row, col)
        cell.formatted_value if cell.respond_to?(:formatted_value)
      end

      def empty?(row, col, sheet = nil)
        sheet = sheet_for(sheet)
        key = normalize(row, col)
        cell = (sheet.lazy_cells || sheet.cells)[key]
        !cell || cell.empty? ||
          (row < sheet.first_row || row > sheet.last_row || col < sheet.first_column || col > sheet.last_column)
      end

      def set(...)
        @calamine_set_calls = (@calamine_set_calls || 0) + 1
        super
      end

      private

      def each_with_headers(headers)
        first, last = headers.map(&:last).minmax
        slots = headers.map { |key, column| [key, column - first] }
        @header_line.upto(last_row) do |line|
          lazy = sheet_for(nil).lazy_cells unless headers.empty?
          yield(lazy ? lazy.hash_row(line, first, last, slots) : cells_header_row(line, headers))
        end
      end

      def cells_header_row(line, headers)
        cells = nil
        headers.each_with_object({}) do |(key, column), row|
          cells ||= sheet_for(nil).cells
          row[key] = safe_send(cells[normalize(line, column)], :value)
        end
      end

      # Roo::Base#each, up to its loop over the rows.
      def header_columns(options)
        clean_sheet_if_need(options)
        build_cells
        search_or_set_header(options)
        @headers || (first_column..last_column).to_h { |column| [cell(@header_line, column), column] }
      end

      # Builds every cell before roo's next step unless the lazy cells answer it. When that raises,
      # roo's own read raises the same error where roo raises it.
      def build_cells
        sheet = sheet_for(nil)
        sheet.cells unless sheet.lazy_cells
      rescue StandardError
        nil
      end

      def rows_for(headers)
        fields = headers.map { |key, column| [key, normalize(0, column).last] }
        sheet_for(nil).parse_rows(@header_line.upto(last_row), fields)
      end
    end
  end
end
