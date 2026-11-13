# frozen_string_literal: true

module Roo
  module Turbo
    # Prepended to Roo::Excelx::SheetDoc. roo finds a sheet's hyperlinks
    # (#hyperlinks, behind every read of a sheet with a hyperlink
    # relationship) and its merged ranges with XPath on the whole sheet.
    # Native.sheet_parts resolves links without a Nokogiri document.
    module SheetParts
      # roo versions whose hyperlink and merge rules these methods reproduce.
      ROO_VERSIONS = %w[4.0.0].freeze

      def hyperlinks(relationships)
        return super unless native? && ROO_VERSIONS.include?(Roo::VERSION)
        return @hyperlinks if @hyperlinks
        return @hyperlinks = {} if @options[:no_hyperlinks]

        targets = relationships.turbo_hyperlink_targets
        return @hyperlinks = {} unless targets
        return super unless (parts = turbo_parts(targets))

        @hyperlinks = parts.first
      end

      private

      def expand_merged_ranges(cells)
        return super unless (parts = turbo_parts)

        merges = {}
        parts.last.each { |ref| merge_range(merges, ref, cells) }
        merges.each { |dst, src| cells[dst] = cells[src] }
      end

      def merge_range(merges, ref, cells)
        src, dst = ref.split(":").map { |key| ::Roo::Utils.ref_to_key(key) }
        return unless cells[src]

        (src[0]..dst[0]).each do |row|
          (src[1]..dst[1]).each do |col|
            merges[[row, col]] = src unless row == src[0] && col == src[1]
          end
        end
      end

      # [hyperlinks, merge_cells] from Native.sheet_parts, or nil when roo
      # reads them from the whole document.
      def turbo_parts(targets = {})
        return unless native? && ROO_VERSIONS.include?(Roo::VERSION)

        @turbo_parts = Native.sheet_parts(sheet_source, targets) if @turbo_parts.nil?
        @turbo_parts || nil
      rescue UnsupportedError
        @turbo_parts = false
        nil
      end
    end

    # roo's relationship nodes stay lazy; hyperlink reads need only targets.
    module HyperlinkRelationships
      def turbo_hyperlink_targets
        return targets_from_roo if @relationships || !doc_exists?

        Native.hyperlink_targets(@path)
      rescue UnsupportedError
        targets_from_roo
      end

      private

      def targets_from_roo
        return unless include_type?("hyperlink")

        to_a.transform_values { |relationship| relationship["Target"] }
      end
    end
  end
end
