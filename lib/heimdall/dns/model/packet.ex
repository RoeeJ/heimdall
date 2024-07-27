defmodule Heimdall.DNS.Model.Packet do
  alias Heimdall.DNS.Model

  @type t() :: %__MODULE__{
          header: Model.Header.t(),
          questions: [Model.Question.t()],
          answers: [Model.ResourceRecord.t()],
          nameservers: [Model.ResourceRecord.t()],
          additional: [Model.ResourceRecord.t()]
        }

  defstruct header: nil,
            questions: [],
            answers: [],
            nameservers: [],
            additional: []
end
