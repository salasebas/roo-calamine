# frozen_string_literal: true

require "roo"
require "roo/excelx"
require_relative "turbo/version"

if Roo.const_defined?(:Calamine, false) && !Roo::Calamine.equal?(Roo::Turbo)
  raise LoadError, "Replace roo-calamine with roo-turbo in your Gemfile before loading roo/turbo"
end

Roo::Calamine = Roo::Turbo unless Roo.const_defined?(:Calamine, false)

module Roo
  # Reads `.xlsx` and `.xlsm` worksheets for Roo::Excelx with a native
  # extension. Loading this file is all it takes; `Roo::Turbo.enabled =
  # false` or `ROO_TURBO=0` switches back to roo's own reader at runtime.
  module Turbo
    # Raised by the native reader for XML it does not reproduce exactly the
    # way roo reads it. Roo::Turbo rescues it and lets roo read the sheet.
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
        environment = ENV.fetch("ROO_TURBO") { ENV.fetch("ROO_CALAMINE", "") }
        @enabled && available? && !DISABLING_ENV_VALUES.include?(environment.downcase)
      end

      # roo-turbo's own API, not roo's: yields each row of a worksheet as an
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
    require "roo/turbo/#{RUBY_VERSION[/\d+\.\d+/]}/roo_turbo"
  rescue LoadError
    require "roo/turbo/roo_turbo"
  end
  require_relative "turbo/context"
  require_relative "turbo/deferred_sheets"
  require_relative "turbo/sheet_doc"
  require_relative "turbo/sheet_parts"
  require_relative "turbo/excelx"
  require_relative "turbo/csv"
  require_relative "turbo/formatters"
  require_relative "turbo/row_index"
  require_relative "turbo/row_iteration"
  require_relative "turbo/sheet"
  require_relative "turbo/shared_strings"
  Roo::Excelx::SheetDoc.prepend(Roo::Turbo::DeferredSheetDoc)
  Roo::Excelx::SheetDoc.prepend(Roo::Turbo::SheetDoc)
  Roo::Excelx::SheetDoc.prepend(Roo::Turbo::SheetParts)
  Roo::Excelx::Relationships.prepend(Roo::Turbo::HyperlinkRelationships)
  Roo::Excelx::Sheet.prepend(Roo::Turbo::Sheet)
  Roo::Excelx.prepend(Roo::Turbo::DeferredSheets)
  Roo::Excelx.prepend(Roo::Turbo::Excelx)
  Roo::Excelx.prepend(Roo::Turbo::SheetSelection)
  Roo::Excelx::SharedStrings.prepend(Roo::Turbo::SharedStrings)
  Roo::Excelx::Workbook.prepend(Roo::Turbo::MetadataDocument)
  Roo::Excelx::Styles.prepend(Roo::Turbo::MetadataDocument)
  Roo::Excelx.prepend(Roo::Turbo::CSV)
  Roo::Excelx.prepend(Roo::Turbo::Formatters)
end
