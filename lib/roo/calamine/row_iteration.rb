# frozen_string_literal: true

module Roo
  module Calamine
    # Roo::Base#each reads the current sheet's row after every block call.
    module Excelx
      private

      def native_rows?
        Calamine.enabled? && method(:row).owner.equal?(::Roo::Excelx)
      end

      def each_without_options(&)
        sheet = sheet_for(nil)
        lazy = sheet.lazy_cells
        return each_lazy_row(sheet, lazy, &) if lazy && sheet.first_column

        index = RowIndex.new(self)
        1.upto(last_row) { |line| yield index.row(line, @calamine_set_calls) }
      end

      def each_lazy_row(sheet, lazy)
        last = last_row
        line = lazy.each_row(last, sheet.first_column, sheet.last_column) do |values|
          yield values
          sheet_for(nil).equal?(sheet) && sheet.lazy_cells.equal?(lazy)
        end
        line.upto(last) { |number| yield row(number) }
        1
      end
    end
  end
end
