# frozen_string_literal: true

module Roo
  module Oxide
    # Prepended to Roo::Excelx::SheetDoc, where every worksheet read in roo
    # starts: Sheet#cells (and with it #row, #each, #parse), the lazy cells
    # behind Excelx#cell and the other single-cell lookups, and Sheet#each_row
    # (#each_row_streaming). Offsets and row limits stay roo's code.
    module SheetDoc
      # Cells the native reader parses ahead of Ruby, per batch.
      BATCH_CELLS = 16_384

      # A sheet already read into a Native::Store is built from it even when
      # the native reader has been turned off since, so the cells handed out
      # stay the ones #cells holds.
      def cells(relationships)
        return super if @cells || !(native? || @oxide_store)

        store = @oxide_store || nil
        @oxide_store = nil
        @cells = store ? store.cells : read_cells(relationships)
      rescue UnsupportedError
        super
      end

      # The cells of #cells, each built when it is first read. nil when they
      # have to be built all at once: when #cells already exists, when merged
      # ranges are expanded, and when building some cell would raise, because
      # roo raises for the whole sheet then.
      def lazy_cells(relationships)
        if @oxide_store.nil? && !@cells && !@options[:expand_merged_ranges] && native?
          @oxide_store = read_store(relationships)
        end
        @oxide_store || nil
      end

      # Yields Native::Row objects; #each_cell turns one into cells when roo
      # asks for it, so rows roo skips are never converted, as in roo.
      def each_row_streaming(&)
        return super unless native?

        yielded = 0
        reader = Native::Reader.new(sheet_source, true, BATCH_CELLS)
        while (row = reader.next_row)
          yield row
          yielded += 1
        end
        nil
      rescue UnsupportedError
        # roo reads the sheet again; skip the rows already yielded.
        skip = yielded
        super { |row| skip.zero? ? yield(row) : skip -= 1 }
      ensure
        # Also when the caller stops early, as roo does for max_rows.
        reader&.close
      end

      def each_cell(row_xml, &)
        return super unless row_xml.is_a?(Native::Row)

        oxide_stream_converter.row_cells(row_xml, false).each(&)
      end

      # The cells #each_cell yields for a Native::Row, as a new Array; nil for
      # rows roo reads itself, custom #each_cell, or roo-only padding.
      def native_row_cells(row, pad_cells: false)
        return unless row.is_a?(Native::Row) && method(:each_cell).owner.equal?(SheetDoc)

        oxide_stream_converter.row_cells(row, pad_cells)
      end

      # Roo::Oxide.each_row_values. `sheet` is the Roo::Excelx::Sheet that
      # owns this document; it turns rows that roo reads itself into cells.
      def each_row_values(sheet, &)
        return each_roo_row_values(sheet, 0, &) unless native?

        yielded = 0
        reader = Native::Reader.new(sheet_source, true, BATCH_CELLS)
        while (values = reader.next_values(oxide_stream_converter))
          yield values
          yielded += 1
        end
        nil
      rescue UnsupportedError
        each_roo_row_values(sheet, yielded, &)
      ensure
        reader&.close
      end

      private

      # The values of the rows roo's own #each_row_streaming yields, after the
      # first `skip`.
      def each_roo_row_values(sheet, skip)
        method(:each_row_streaming).super_method.call do |row|
          next skip -= 1 unless skip.zero?

          yield sheet.send(:cells_for_row_element, row, pad_cells: true).map { |cell| cell&.value }
        end
        nil
      end

      def oxide_stream_converter
        @oxide_stream_converter ||= Native::Converter.new(Context.new(@shared, hyperlinks(@relationships), true),
                                                          true)
      end

      def native?
        @path && Oxide.enabled?
      end

      # Every cell, in roo's order.
      def read_cells(relationships)
        cells = {}
        reader = Native::Reader.new(sheet_source, false, BATCH_CELLS)
        converter = Native::Converter.new(context(relationships), false)
        nil while reader.fill_cells(cells, converter)
        expand_merged_ranges(cells) if @options[:expand_merged_ranges]
        cells
      ensure
        reader&.close
      end

      # A Native::Store, or false when the cells have to be built all at once.
      # roo applies the hyperlinks to every cell on the first read, so later
      # changes to the hash it hands out do not reach the cells.
      def read_store(relationships)
        Native::Store.read(sheet_source, context(relationships, hyperlinks(relationships).dup)) || false
      rescue UnsupportedError
        false
      end

      def context(relationships, links = hyperlinks(relationships))
        Context.new(@shared, links, @options[:empty_cell])
      end
    end
  end
end
