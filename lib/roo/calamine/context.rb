# frozen_string_literal: true

module Roo
  module Calamine
    # The answers the native reader needs from roo while it builds cells. Each
    # one comes from the roo object or method that SheetDoc#cell_from_xml uses,
    # so formats, shared strings, and errors are roo's own.
    class Context
      attr_reader :hyperlinks, :empty_cell

      def initialize(shared, hyperlinks, empty_cell)
        @shared = shared
        @hyperlinks = hyperlinks
        @empty_cell = empty_cell
      end

      def base_date
        @shared.base_date
      end

      def base_timestamp
        @shared.base_timestamp
      end

      def style(index)
        format = @shared.styles.style_format(index)
        [format.to_s, Roo::Excelx::Format.to_type(format)]
      end

      def style_index(text)
        text.to_i
      end

      def coordinate(reference)
        Roo::Utils.extract_coordinate(reference)
      end

      # The value of every shared string, by index.
      def shared_strings
        strings = @shared.shared_strings
        Array.new(strings.to_a.size) { |index| shared_string_at(strings, index) }
      end

      def shared_string(text)
        shared_string_at(@shared.shared_strings, text.to_i)
      end

      private

      def shared_string_at(strings, index)
        strings.use_html?(index) ? strings.to_html[index] : strings[index]
      end
    end
  end
end
