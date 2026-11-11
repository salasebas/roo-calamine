# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::SheetDoc. roo finds a sheet's hyperlinks
    # (#hyperlinks, behind every read of a sheet with a hyperlink
    # relationship) and its merged ranges (#expand_merged_ranges, under the
    # option of that name) with XPath on a Nokogiri document of the whole
    # sheet. Native.sheet_parts reads those elements alone, and copies of roo's
    # loops turn them into the same results.
    module SheetParts
      # roo versions whose #extract_hyperlinks and #expand_merged_ranges these
      # methods copy; under any other version roo reads the whole document.
      ROO_VERSIONS = %w[4.0.0].freeze

      private

      def extract_hyperlinks(relationships)
        return super unless (parts = calamine_parts)

        parts.first.each_with_object({}) do |(ref, id, location), hash|
          next unless (relationship = relationships[id])

          target_link = relationship["Target"]
          target_link += "##{location}" if location
          Roo::Utils.coordinates_in_range(ref.to_s) { |coord| hash[coord] = target_link }
        end
      end

      def expand_merged_ranges(cells)
        return super unless (parts = calamine_parts)

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
      def calamine_parts
        return unless native? && ROO_VERSIONS.include?(Roo::VERSION)

        @calamine_parts = Native.sheet_parts(sheet_source) if @calamine_parts.nil?
        @calamine_parts || nil
      rescue UnsupportedError
        @calamine_parts = false
        nil
      end
    end
  end
end
