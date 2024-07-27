defmodule Heimdall.DNS.Model.Question do
  @moduledoc """
  Represents a DNS question.
  """

  alias Heimdall.DNS.{Model}

  @type t() :: %__MODULE__{
          qname: String.t(),
          qclass: Model.qclass_atoms(),
          qtype: Model.qtype_atoms()
        }

  defstruct qname: nil, qtype: nil, qclass: nil
end
