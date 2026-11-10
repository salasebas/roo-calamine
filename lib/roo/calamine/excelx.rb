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
    # #each without options reads its rows through Sheet#row, or through a
    # RowIndex once every cell is built.
    module Excelx
      def each(options = {}, &)
        return super unless block_given?

        if options.empty?
          build_cells(lazy: true)
          return row_index? ? each_without_options(&) : super
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

        rows = rows_for(header_columns(options, lazy: true))
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

      def set(...)
        @calamine_set_calls = (@calamine_set_calls || 0) + 1
        super
      end

      private

      # Roo::Base#each without options: `1.upto(last_row) { |line| yield row(line) }`.
      def each_without_options
        index = RowIndex.new(self)
        1.upto(last_row) { |line| yield index.row(line, @calamine_set_calls) }
      end

      def row_index?
        Calamine.enabled? && !sheet_for(nil).lazy_cells && method(:row).owner.equal?(::Roo::Excelx)
      end

      # Roo::Base#each, up to its loop over the rows.
      def header_columns(options, lazy: false)
        clean_sheet_if_need(options)
        build_cells(lazy:)
        search_or_set_header(options)
        @headers || (first_column..last_column).to_h { |column| [cell(@header_line, column), column] }
      end

      # Builds every cell before roo's next step, which reads them all, unless
      # it reads them `lazy` and the lazy cells answer it. When that raises,
      # roo's own read raises the same error where roo raises it.
      def build_cells(lazy: false)
        sheet = sheet_for(nil)
        sheet.cells unless lazy && sheet.lazy_cells
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
