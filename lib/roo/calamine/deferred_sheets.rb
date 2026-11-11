# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx. roo extracts every worksheet to its temporary
    # directory when it opens a workbook. With the native reader on, a
    # workbook opened from a path leaves its worksheets in the zip instead:
    # the native reader inflates the one it reads, and roo's own code finds
    # the file extracted the first time it reads it (DeferredSheetDoc). roo
    # reads the workbook's file only while it opens it, so it opens a copy in
    # its temporary directory, and the workbook reads the same after its file
    # is changed, moved, or deleted. The native extension also writes the
    # files roo does extract, such as sharedStrings.xml: rubyzip's inflating
    # leaves several times the file's size in short-lived Strings, which
    # Ruby's GC frees only after they have grown the process.
    module DeferredSheets
      # roo versions whose #extract_sheets_in_order this reproduces.
      ROO_VERSIONS = %w[4.0.0].freeze
      COMPRESSION_METHODS = [::Zip::Entry::STORED, ::Zip::Entry::DEFLATED].freeze
      private_constant :ROO_VERSIONS, :COMPRESSION_METHODS

      # `path` as the native extension takes it. On Windows, Ruby opens a
      # binary path's bytes as UTF-8, where magnus refuses a binary string.
      # The extension raises EncodingError for a path it cannot take.
      def self.native_path(path)
        path = ::File.path(path)
        return path unless Gem.win_platform? && path.encoding == Encoding::BINARY

        path.dup.force_encoding(Encoding::UTF_8)
      end

      # Extended onto each Zip::Entry of the copy that the native extension
      # can read: `calamine_entry` is its Native::ZipEntry, which writes the
      # file when roo extracts the entry. When the native extraction fails,
      # or cannot take the path, rubyzip extracts it as roo does, and raises
      # or warns as roo does.
      module NativeEntry
        attr_accessor :calamine_entry

        def extract(entry_path, destination_directory:)
          calamine_entry.extract(DeferredSheets.native_path(::File.join(destination_directory, entry_path)))
          self
        rescue IOError, EncodingError
          super
        end
      end

      # A worksheet left in the zip: `zip_entry` is rubyzip's Zip::Entry for
      # it, a NativeEntry.
      class Sheet
        def initialize(zip_entry)
          @zip_entry = zip_entry
          @lock = Mutex.new
          @extracted = false
        end

        # Its Native::ZipEntry.
        def entry
          @zip_entry.calamine_entry
        end

        # Extracts the worksheet where roo would have, once; another thread
        # waits until the file is complete.
        def extract(path)
          @lock.synchronize do
            next if @extracted

            @zip_entry.extract(::File.basename(path), destination_directory: ::File.dirname(path))
            @extracted = true
          end
        end
      end

      private

      def process_zipfile(zipfilename_or_stream)
        @calamine_copy = calamine_copy
        @calamine_copy ? super(@calamine_copy) : super
      end

      # The workbook's file for roo to open: a copy in roo's temporary
      # directory, or the file roo wrote there itself for a URL or
      # `packed: :zip`. An empty file is left to roo: rubyzip's error names
      # the file it opened.
      def calamine_copy
        return unless @filename && Calamine.enabled? && ROO_VERSIONS.include?(Roo::VERSION) && ::File.size?(@filename)
        return @filename if ::File.dirname(@filename) == @tmpdir

        "#{@tmpdir}/roo_calamine.xlsx".tap { IO.copy_stream(@filename, _1) }
      end

      def process_zipfile_entries(entries)
        native_entries(entries) if @calamine_copy
        super
      end

      # Makes each entry the native extension can read a NativeEntry. roo
      # raises or warns while extracting an entry that is not a file,
      # encrypted, or compressed by another method; rubyzip still extracts it.
      def native_entries(entries)
        zip = DeferredSheets.native_path(@calamine_copy)
        entries.each do |entry|
          next unless entry.file? && !entry.encrypted? && COMPRESSION_METHODS.include?(entry.compression_method)

          entry.extend(NativeEntry).calamine_entry =
            Native::ZipEntry.new(zip, entry.local_header_offset, entry.compressed_size, entry.size,
                                 entry.compression_method)
        end
      rescue EncodingError
        nil
      end

      # Roo::Excelx#extract_sheets_in_order, without the extraction. roo
      # raises or warns reading a worksheet that is missing or that the
      # native extension cannot read; it still does.
      def extract_sheets_in_order(entries, sheet_ids, sheets, tmpdir)
        found = (sheet_ids & sheets.keys).map { |id| sheet_entry(entries, sheets[id]) }
        return super unless found.all?(NativeEntry)

        deferred = found.each_with_index.to_h { |entry, i| ["#{tmpdir}/roo_sheet#{i + 1}", Sheet.new(entry)] }
        sheet_files.concat(deferred.keys)
        @sheet_files.concat(deferred.keys)
        @shared.instance_variable_set(:@calamine_deferred_sheets, deferred)
      end

      def sheet_entry(entries, name)
        name && entries.find { |e| "/#{e.name}".end_with?(name) }
      end
    end

    # Prepended to Roo::Excelx::SheetDoc, below Roo::Calamine::SheetDoc. roo
    # reads a worksheet's file in #doc, #dimensions, and #each_row_streaming;
    # each one extracts a deferred worksheet first.
    module DeferredSheetDoc
      def dimensions
        extract_deferred_sheet
        super
      end

      def each_row_streaming(&)
        extract_deferred_sheet
        super
      end

      private

      def doc
        extract_deferred_sheet
        super
      end

      # What the native reader reads: the worksheet's Native::ZipEntry, or
      # its file when it was not left in the zip.
      def sheet_source
        deferred_sheet&.entry || @path
      end

      def extract_deferred_sheet
        deferred_sheet&.extract(@path)
      end

      # nil once the workbook is closed or collected: its temporary directory
      # is gone, and roo raises reading the worksheet's file.
      def deferred_sheet
        sheet = @shared.instance_variable_get(:@calamine_deferred_sheets)&.[](@path)
        sheet if sheet && ::File.directory?(::File.dirname(@path))
      end
    end
  end
end
