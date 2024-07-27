defmodule Heimdall.DNS.Server do
  @moduledoc """
  DNS Server module for handling DNS queries and responses.
  """

  require Logger
  alias Heimdall.DNS.{Encoder, Decoder, Model, Resolver}

  def query(data) do
    with {:ok, %Model.Packet{} = packet} <- Decoder.packet(data),
         %Model.Packet{} = response <- process_packet(packet),
         {:ok, encoded_response} <- Encoder.packet(response) do
      {:ok, encoded_response}
    else
      {:error, reason} ->
        {:error, reason}
    end
  end

  defp process_packet(%Model.Packet{} = packet) do
    answers = resolve_queries(packet.questions)
    # process_additional(packet.additional)
    additional = []

    # TODO: replace hardcoded values with constants
    %Model.Packet{
      packet
      | header: %Model.Header{
          packet.header
          | qr: true,
            recurs_available: true,
            resp_code: if(answers == [], do: :nxdomain, else: packet.header.resp_code)
        },
        answers: answers,
        additional: additional
    }
  end

  defp resolve_queries(questions) do
    Enum.flat_map(questions, fn %Model.Question{} = q ->
      case Resolver.query(q.qname, q.qtype) do
        {:ok, resources} ->
          resources

        {:error, reason} ->
          Logger.debug("Got err: #{reason} when querying #{q.qname} for #{q.qtype} records")
          []
      end
    end)
  end
end
