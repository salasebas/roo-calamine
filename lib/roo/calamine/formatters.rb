# frozen_string_literal: true

module Roo
  module Calamine
    # Roo 4.0.0's whole-sheet output, using the native records while the
    # sheet's public cells hash has not been materialized.
    module Formatters
      def to_yaml(prefix = {}, from_row = nil, from_column = nil, to_row = nil, to_column = nil, sheet = default_sheet) # rubocop:disable Metrics/ParameterLists
        return super unless native_formatters?
        return "" unless first_row

        area = formatter_area(sheet, [from_row, from_column, to_row, to_column])
        store = sheet_for(sheet).lazy_cells
        return super unless store && area.all?(Integer)

        store.to_yaml(self, prefix, area)
      end

      def to_matrix(from_row = nil, from_column = nil, to_row = nil, to_column = nil, sheet = default_sheet) # rubocop:disable Metrics/ParameterLists
        return super unless native_formatters?

        require "matrix"
        return ::Matrix.empty unless first_row

        area = formatter_rectangle(sheet, [from_row, from_column, to_row, to_column])
        store = sheet_for(sheet).lazy_cells
        return super unless store && area.all?(Integer)

        ::Matrix.rows(store.matrix_rows(area))
      end

      def to_xml
        return super unless native_formatters?
        return super if sheets.empty?

        targets = sheets.map { |name| sheet_for(name) }
        return super unless targets.all?(&:lazy_cells)

        result = +"<?xml version=\"1.0\"?>\n<spreadsheet>\n"
        document = Nokogiri::XML::Document.new
        sheets.zip(targets).each do |name, target|
          self.default_sheet = name
          result << formatter_xml_sheet(document, name, target)
        end
        result << "</spreadsheet>\n"
      end

      private

      def native_formatters?
        Calamine.enabled? && Roo::VERSION == "4.0.0"
      end

      def formatter_area(sheet, rectangle)
        target = sheet_for(sheet)
        formatter_rectangle(sheet, rectangle) +
          [target.first_row, target.first_column, target.last_row, target.last_column]
      end

      def formatter_rectangle(sheet, rectangle)
        bounds = %i[first_row first_column last_row last_column]
        rectangle.zip(bounds).map { |requested, bound| requested || public_send(bound, sheet) }
      end

      def formatter_xml_sheet(document, name, target)
        element = document.create_element("sheet", name:)
        cells = first_row ? target.lazy_cells.xml_cells(self, formatter_area(name, [nil] * 4)) : ""
        "  #{element.to_xml.delete_suffix("/>")}#{cells.empty? ? "/>\n" : ">\n#{cells}  </sheet>\n"}"
      end

      def formatter_yaml_prefix(prefix)
        text = +""
        prefix.each { |key, value| text << "  #{key}: #{value} \n" }
        text
      end

      def formatter_xml_cell(value, row, column, type)
        Nokogiri::XML::Document.new.create_element("cell", value, row:, column:, type:).to_xml
      end
    end
  end
end
