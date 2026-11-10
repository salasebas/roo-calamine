# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::SheetDoc, where every worksheet read in roo
    # starts: Sheet#cells (and with it #cell, #row, #each, #parse) and
    # Sheet#each_row (#each_row_streaming). Everything else, including roo's
    # padding, offset, and row limits, stays roo's code.
    module SheetDoc
      DOCUMENT_BATCH_ROWS = 1024
      STREAM_BATCH_ROWS = 128

      def cells(relationships)
        return super if @cells || !@path || !Calamine.enabled?

        cells = {}
        reader = Native::Reader.new(@path, false)
        context = Context.new(@shared, hyperlinks(relationships), @options[:empty_cell])
        nil while reader.fill_cells(cells, context, DOCUMENT_BATCH_ROWS)
        expand_merged_ranges(cells) if @options[:expand_merged_ranges]
        @cells = cells
      rescue UnsupportedError
        super
      end

      def each_row_streaming(&)
        return super unless @path && Calamine.enabled?

        yielded = 0
        reader = Native::Reader.new(@path, true)
        context = Context.new(@shared, hyperlinks(@relationships), true)
        while (rows = reader.next_rows(context, STREAM_BATCH_ROWS))
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
        return super unless row_xml.is_a?(Array)

        row_xml.each(&)
      end
    end
  end
end
