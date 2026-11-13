# frozen_string_literal: true

require "English"

module Roo
  module Turbo
    # Prepended to Roo::Excelx. roo's #to_csv writes every slot from the first
    # row and column to the last through #empty?, #cell and #celltype, and each
    # of them looks up the sheet and the cell again. This writes the same text
    # from one pass over the sheet's cells: natively from the lazy cells while
    # the sheet has them, else a line at a time, asking each cell what roo's
    # Roo::Formatters::CSV#cell_to_csv asks it, in the same order.
    module CSV
      # roo versions whose Roo::Formatters::CSV this reproduces. Under any other
      # version roo writes the CSV itself.
      ROO_VERSIONS = %w[4.0.0].freeze
      private_constant :ROO_VERSIONS

      private

      def write_csv_content(file = nil, sheet = nil, separator = ",")
        return super unless csv_lines?(separator.to_s)

        file ||= STDOUT # rubocop:disable Style/GlobalStdStream
        return unless first_row(sheet)

        target = sheet_for(sheet)
        area = [last_row(sheet), last_column(sheet), target.first_row, target.last_row, target.first_column,
                target.last_column]
        store = target.lazy_cells
        if store && area.all?(Integer)
          store.write_csv(file, self, separator.to_s, area)
        else
          write_csv_lines(file, target, separator.to_s, *area.first(2))
        end
      end

      # roo prints each field on its own; a line joins them, so the separator
      # has to join UTF-8 text, and IO#print must add no $\ between fields.
      def csv_lines?(separator)
        Turbo.enabled? && ROO_VERSIONS.include?(Roo::VERSION) && $OUTPUT_RECORD_SEPARATOR.nil? &&
          (separator.ascii_only? || separator.encoding == Encoding::UTF_8)
      end

      # roo writes rows 1..last_row and columns 1..last_column, and prints the
      # separator before every column but the first, then the field, so after
      # a field raises the file holds the line up to it.
      def write_csv_lines(file, sheet, separator, rows, width) # rubocop:disable Metrics
        first_row = sheet.first_row
        last_row = sheet.last_row
        first_column = sheet.first_column
        last_column = sheet.last_column
        separators = Hash.new { |runs, count| runs[count] = separator * count }
        blank_line = "#{separators[[width - 1, 0].max]}\n"
        line = +""
        line_row = written = 0
        each_csv_cell(sheet.cells, rows, width) do |row, column, cell|
          if row != line_row
            file.print(line << separators[width - 1 - written] << "\n") if line_row.positive?
            (row - line_row - 1).times { file.print(blank_line) }
            line.clear
            line_row = row
            written = 0
          end
          line << separators[column - 1 - written]
          written = column - 1
          begin
            next if !cell || cell.empty? || row < first_row || row > last_row ||
                    column < first_column || column > last_column

            line << csv_field(cell)
          rescue StandardError
            file.print(line)
            raise
          end
        end
        file.print(line << separators[width - 1 - written] << "\n") if line_row.positive?
        (line_row + 1).upto(rows) { file.print(blank_line) }
      end

      # Yields the cells roo visits, by row and then column: those with an
      # Integer row in 1..rows and an Integer column in 1..columns.
      def each_csv_cell(cells, rows, columns)
        if csv_keys_sorted?(cells)
          cells.each_pair do |(row, column), cell|
            break if row > rows

            yield row, column, cell if row.positive? && column.between?(1, columns)
          end
        else
          sorted_csv_keys(cells, rows, columns).each { |key| yield key[0], key[1], cells[key] }
        end
      end

      def sorted_csv_keys(cells, rows, columns)
        keys = cells.keys.select do |key|
          key.is_a?(Array) && key.size == 2 && key.all?(Integer) &&
            key[0].between?(1, rows) && key[1].between?(1, columns)
        end
        keys.sort!
      end

      # Whether every key is a [row, column] of Integers, in increasing order
      # from row 0.
      def csv_keys_sorted?(cells) # rubocop:disable Metrics/CyclomaticComplexity, Metrics/PerceivedComplexity
        previous_row = previous_column = 0
        cells.each_key do |key|
          return false unless key.is_a?(Array) && key.size == 2

          row, column = key
          return false unless row.is_a?(Integer) && column.is_a?(Integer)

          if row == previous_row
            return false if column < previous_column
          elsif row < previous_row
            return false
          end
          previous_row = row
          previous_column = column
        end
        true
      end

      # Roo::Formatters::CSV#cell_to_csv after its #empty? check.
      def csv_field(cell) # rubocop:disable Metrics/CyclomaticComplexity
        value = cell.value
        case (type = cell.type)
        when :string then csv_quote(value) unless value.empty?
        when :boolean then %("#{cell.formatted_value.gsub('"', '""').downcase}")
        when :float, :percentage then csv_number(value)
        when :formula then csv_formula_value(value)
        when :date, :datetime then value.to_s
        when :time then integer_to_timestring(value)
        when :link then csv_quote(value.url)
        else raise "unhandled celltype #{type}"
        end || ""
      end

      def csv_formula_value(value)
        case value
        when String then csv_quote(value) unless value.empty?
        when Integer, Date, DateTime, TrueClass, FalseClass then value.to_s
        when Float then csv_number(value)
        else raise "unhandled onecell-class #{value.class}"
        end
      end

      def csv_quote(text)
        text.include?('"') ? %("#{text.gsub('"', '""')}") : %("#{text}")
      end

      def csv_number(value)
        value == value.to_i ? value.to_i.to_s : value.to_s
      end
    end
  end
end
