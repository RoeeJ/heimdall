defmodule Heimdall.DNS.Model.ResourceRecord do
  @moduledoc """
  Resource record for DNS.
  """

  alias Heimdall.DNS.{Model}

  @type t :: %__MODULE__{
          qname: String.t(),
          qtype: Model.qtype_atoms(),
          qclass: Model.qclass_atoms() | non_neg_integer(),
          ttl: non_neg_integer(),
          rdlength: non_neg_integer(),
          rdata: Model.EDNS.t() | bitstring() | any()
        }

  defstruct qname: nil, qtype: nil, qclass: :in, ttl: 0, rdlength: nil, rdata: nil
end
