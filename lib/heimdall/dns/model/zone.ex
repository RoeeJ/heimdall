defmodule Heimdall.DNS.Model.Zone do
  alias Heimdall.DNS.Model
  @type t() :: %__MODULE__{domain: String.t(), records: [Model.ResourceRecord.t()]}
  defstruct domain: nil, records: []
end
