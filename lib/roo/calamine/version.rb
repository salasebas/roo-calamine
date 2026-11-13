# frozen_string_literal: true

require_relative "../oxide/version"

Roo::Calamine = Roo::Oxide unless Roo.const_defined?(:Calamine, false)
