# frozen_string_literal: true

module Roo
  module Calamine
    # The answers the native reader needs from roo while it builds cells. Each
    # one comes from the roo object or method that SheetDoc#cell_from_xml uses,
    # so formats, shared strings, and errors are roo's own.
    class Context
      # roo versions whose cell constructors the native reader reproduces:
      # under them it sets each cell's instance variables itself, and under
      # any other version it calls the constructors.
      CELL_LAYOUT_ROO_VERSIONS = %w[4.0.0].freeze

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

      def build_cells?
        CELL_LAYOUT_ROO_VERSIONS.include?(Roo::VERSION)
      end

      # The format string, its type, and whether Cell::Number#create_numeric
      # always calls Float under it (nil when matching it raises).
      def style(index)
        format = @shared.styles.style_format(index)
        [format.to_s, Roo::Excelx::Format.to_type(format), float_format(format.to_s)]
      end

      def style_index(text)
        text.to_i
      end

      def coordinate(reference)
        Roo::Utils.extract_coordinate(reference)
      end

      # Roo's plain strings and deferred or public HTML table, nil when disabled.
      def shared_strings
        strings = @shared.shared_strings
        html_disabled = strings.instance_variable_get(:@options)[:disable_html_wrapper]
        plain = strings.to_a
        [plain, (strings.calamine_html unless html_disabled)]
      end

      def shared_string(text)
        shared_string_at(@shared.shared_strings, text.to_i)
      end

      private

      def float_format(format)
        format.match?(/%|\.0/)
      rescue ArgumentError, EncodingError
        nil
      end

      def shared_string_at(strings, index)
        strings.use_html?(index) ? strings.to_html[index] : strings[index]
      end
    end
  end
end
