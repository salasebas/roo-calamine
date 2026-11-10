# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::SheetDoc, where every worksheet read in roo
    # starts: Sheet#cells (and with it #cell, #row, #each, #parse) and
    # Sheet#each_row (#each_row_streaming). Everything else, including roo's
    # padding, offset, and row limits, stays roo's code.
    module SheetDoc
      # Cells parsed per call into the extension, with the GVL released. Fewer,
      # larger batches mean fewer GVL handoffs, each of which can wait a full
      # thread time slice when another Ruby thread is busy.
      BATCH_CELLS = 65_536

      def cells(relationships)
        return super if @cells || !@path || !Calamine.enabled?

        cells = {}
        reader = Native::Reader.new(@path, false)
        converter = Native::Converter.new(Context.new(@shared, hyperlinks(relationships), @options[:empty_cell]), false)
        nil while reader.fill_cells(cells, converter, BATCH_CELLS)
        expand_merged_ranges(cells) if @options[:expand_merged_ranges]
        @cells = cells
      rescue UnsupportedError
        super
      end

      # Yields Native::Row objects; #each_cell turns one into cells when roo
      # asks for it, so rows roo skips are never converted, as in roo.
      def each_row_streaming(&)
        return super unless @path && Calamine.enabled?

        yielded = 0
        reader = Native::Reader.new(@path, true)
        while (rows = reader.next_rows(BATCH_CELLS))
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
      end

      def each_cell(row_xml, &)
        return super unless row_xml.is_a?(Native::Row)

        @calamine_stream_converter ||= Native::Converter.new(Context.new(@shared, hyperlinks(@relationships), true),
                                                             true)
        @calamine_stream_converter.row_cells(row_xml).each(&)
      end
    end
  end
end
