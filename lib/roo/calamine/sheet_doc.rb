# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::SheetDoc, where every worksheet read in roo
    # starts: Sheet#cells (and with it #row, #each, #parse), the lazy cells
    # behind Excelx#cell and the other single-cell lookups, and Sheet#each_row
    # (#each_row_streaming). Everything else, including roo's padding, offset,
    # and row limits, stays roo's code.
    module SheetDoc
      # Cells the native reader parses ahead of Ruby, per batch.
      BATCH_CELLS = 16_384

      # A sheet already read into a Native::Store is built from it even when
      # the native reader has been turned off since, so the cells handed out
      # stay the ones #cells holds.
      def cells(relationships)
        return super if @cells || !(native? || @calamine_store)

        store = @calamine_store || nil
        @calamine_store = nil
        @cells = read_cells(relationships, store)
      rescue UnsupportedError
        super
      end

      # The cells of #cells, each built when it is first read. nil when they
      # have to be built all at once: when #cells already exists, when merged
      # ranges are expanded, and when building some cell would raise, because
      # roo raises for the whole sheet then.
      def lazy_cells(relationships)
        if @calamine_store.nil? && !@cells && !@options[:expand_merged_ranges] && native?
          @calamine_store = read_store(relationships)
        end
        @calamine_store || nil
      end

      # Yields Native::Row objects; #each_cell turns one into cells when roo
      # asks for it, so rows roo skips are never converted, as in roo.
      def each_row_streaming(&)
        return super unless native?

        yielded = 0
        reader = Native::Reader.new(@path, true, BATCH_CELLS)
        while (rows = reader.next_rows)
          rows.each do |row|
            yield row
            yielded += 1
          end
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

        calamine_stream_converter.row_cells(row_xml).each(&)
      end

      # Roo::Calamine.each_row_values. `sheet` is the Roo::Excelx::Sheet that
      # owns this document; it turns rows that roo reads itself into cells.
      def each_row_values(sheet)
        each_row_streaming do |row|
          yield(if row.is_a?(Native::Row)
                  calamine_stream_converter.row_values(row)
                else
                  sheet.send(:cells_for_row_element, row, pad_cells: true).map { |cell| cell&.value }
                end)
        end
      end

      private

      def calamine_stream_converter
        @calamine_stream_converter ||= Native::Converter.new(Context.new(@shared, hyperlinks(@relationships), true),
                                                             true)
      end

      def native?
        @path && Calamine.enabled?
      end

      # Every cell, in roo's order. With a Native::Store, its converter builds
      # them and the cells it built go in their places.
      def read_cells(relationships, store)
        cells = {}
        reader = Native::Reader.new(@path, false, BATCH_CELLS)
        converter = store ? store.converter : Native::Converter.new(context(relationships), false)
        nil while reader.fill_cells(cells, converter, store)
        expand_merged_ranges(cells) if @options[:expand_merged_ranges]
        cells
      ensure
        reader&.close
      end

      # A Native::Store, or false when the cells have to be built all at once.
      # roo applies the hyperlinks to every cell on the first read, so later
      # changes to the hash it hands out do not reach the cells.
      def read_store(relationships)
        Native::Store.read(@path, context(relationships, hyperlinks(relationships).dup)) || false
      rescue UnsupportedError
        false
      end

      def context(relationships, links = hyperlinks(relationships))
        Context.new(@shared, links, @options[:empty_cell])
      end
    end
  end
end
