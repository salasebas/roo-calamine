# frozen_string_literal: true

require "roo"
require "roo/excelx"
require_relative "calamine/version"

module Roo
  # Reads `.xlsx` and `.xlsm` worksheets for Roo::Excelx with a native
  # extension. Loading this file is all it takes; `Roo::Calamine.enabled =
  # false` or `ROO_CALAMINE=0` switches back to roo's own reader at runtime.
  module Calamine
    # Raised by the native reader for XML it does not reproduce exactly the
    # way roo reads it. Roo::Calamine rescues it and lets roo read the sheet.
    class UnsupportedError < StandardError; end

    DISABLING_ENV_VALUES = %w[0 false off].freeze
    private_constant :DISABLING_ENV_VALUES

    @enabled = true

    class << self
      attr_writer :enabled

      # Whether the native extension is loaded. It is `false` on Ruby
      # implementations other than CRuby, such as JRuby.
      def available?
        defined?(Native::Reader) ? true : false
      end

      # Whether the next worksheet read goes through the native reader.
      def enabled?
        @enabled && available? && !DISABLING_ENV_VALUES.include?(ENV.fetch("ROO_CALAMINE", "").downcase)
      end

      # roo-calamine's own API, not roo's: yields each row of a worksheet as an
      # Array of the values roo's cells return from `#value`, without building
      # the cells. A row is what
      # `Roo::Excelx#each_row_streaming(sheet:, pad_cells: true)` yields, with
      # `cell&.value` in place of each cell: one entry per column from A up to
      # the row's last cell, `nil` where the worksheet has no cell.
      #
      # `source` and `options` are what `Roo::Excelx.new` takes. The workbook
      # is closed when the block returns. Without a block, returns an
      # Enumerator.
      def each_row_values(source, sheet: nil, **options, &block)
        return enum_for(__method__, source, sheet:, **options) unless block

        workbook = Roo::Excelx.new(source, options)
        roo_sheet = workbook.sheet_for(sheet)
        if available?
          roo_sheet.instance_variable_get(:@sheet).each_row_values(roo_sheet, &block)
        else
          roo_sheet.each_row(pad_cells: true) { |cells| yield cells.map { |cell| cell&.value } }
        end
        nil
      ensure
        workbook&.close
      end
    end
  end
end

if RUBY_ENGINE == "ruby"
  begin
    require "roo/calamine/#{RUBY_VERSION[/\d+\.\d+/]}/roo_calamine"
  rescue LoadError
    require "roo/calamine/roo_calamine"
  end
  require_relative "calamine/context"
  require_relative "calamine/deferred_sheets"
  require_relative "calamine/sheet_doc"
  require_relative "calamine/sheet_parts"
  require_relative "calamine/excelx"
  require_relative "calamine/csv"
  require_relative "calamine/formatters"
  require_relative "calamine/row_index"
  require_relative "calamine/row_iteration"
  require_relative "calamine/sheet"
  require_relative "calamine/shared_strings"
  Roo::Excelx::SheetDoc.prepend(Roo::Calamine::DeferredSheetDoc)
  Roo::Excelx::SheetDoc.prepend(Roo::Calamine::SheetDoc)
  Roo::Excelx::SheetDoc.prepend(Roo::Calamine::SheetParts)
  Roo::Excelx::Relationships.prepend(Roo::Calamine::HyperlinkRelationships)
  Roo::Excelx::Sheet.prepend(Roo::Calamine::Sheet)
  Roo::Excelx.prepend(Roo::Calamine::DeferredSheets)
  Roo::Excelx.prepend(Roo::Calamine::Excelx)
  Roo::Excelx::SharedStrings.prepend(Roo::Calamine::SharedStrings)
  Roo::Excelx::Workbook.prepend(Roo::Calamine::MetadataDocument)
  Roo::Excelx::Styles.prepend(Roo::Calamine::MetadataDocument)
  Roo::Excelx.prepend(Roo::Calamine::CSV)
  Roo::Excelx.prepend(Roo::Calamine::Formatters)
end
