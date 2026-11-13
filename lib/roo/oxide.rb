# frozen_string_literal: true

require "roo"
require "roo/excelx"
require_relative "oxide/version"

if Roo.const_defined?(:Calamine, false) && !Roo::Calamine.equal?(Roo::Oxide)
  raise LoadError, "Replace roo-calamine with roo-oxide in your Gemfile before loading roo/oxide"
end

Roo::Calamine = Roo::Oxide unless Roo.const_defined?(:Calamine, false)

module Roo
  # Reads `.xlsx` and `.xlsm` worksheets for Roo::Excelx with a native
  # extension. Loading this file is all it takes; `Roo::Oxide.enabled =
  # false` or `ROO_OXIDE=0` switches back to roo's own reader at runtime.
  module Oxide
    # Raised by the native reader for XML it does not reproduce exactly the
    # way roo reads it. Roo::Oxide rescues it and lets roo read the sheet.
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
        environment = ENV.fetch("ROO_OXIDE") { ENV.fetch("ROO_CALAMINE", "") }
        @enabled && available? && !DISABLING_ENV_VALUES.include?(environment.downcase)
      end

      # roo-oxide's own API, not roo's: yields each row of a worksheet as an
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
    require "roo/oxide/#{RUBY_VERSION[/\d+\.\d+/]}/roo_oxide"
  rescue LoadError
    require "roo/oxide/roo_oxide"
  end
  require_relative "oxide/context"
  require_relative "oxide/deferred_sheets"
  require_relative "oxide/sheet_doc"
  require_relative "oxide/sheet_parts"
  require_relative "oxide/excelx"
  require_relative "oxide/csv"
  require_relative "oxide/formatters"
  require_relative "oxide/row_index"
  require_relative "oxide/row_iteration"
  require_relative "oxide/sheet"
  require_relative "oxide/shared_strings"
  Roo::Excelx::SheetDoc.prepend(Roo::Oxide::DeferredSheetDoc)
  Roo::Excelx::SheetDoc.prepend(Roo::Oxide::SheetDoc)
  Roo::Excelx::SheetDoc.prepend(Roo::Oxide::SheetParts)
  Roo::Excelx::Relationships.prepend(Roo::Oxide::HyperlinkRelationships)
  Roo::Excelx::Sheet.prepend(Roo::Oxide::Sheet)
  Roo::Excelx.prepend(Roo::Oxide::DeferredSheets)
  Roo::Excelx.prepend(Roo::Oxide::Excelx)
  Roo::Excelx.prepend(Roo::Oxide::SheetSelection)
  Roo::Excelx::SharedStrings.prepend(Roo::Oxide::SharedStrings)
  Roo::Excelx::Workbook.prepend(Roo::Oxide::MetadataDocument)
  Roo::Excelx::Styles.prepend(Roo::Oxide::MetadataDocument)
  Roo::Excelx.prepend(Roo::Oxide::CSV)
  Roo::Excelx.prepend(Roo::Oxide::Formatters)
end
