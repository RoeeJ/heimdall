defmodule Heimdall.DNS.Server do
  @moduledoc """
  DNS Server module for handling DNS queries and responses.
  """

  require Logger
  alias Heimdall.DNS.Model.EDNS
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
    additional = Enum.map(packet.additional, &process_additional/1)

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

  defp process_additional(%Model.ResourceRecord{qtype: :opt} = additional) do
    updated_options =
      Enum.map(additional.rdata.options, fn
        {:cookie, client_cookie} ->
          server_cookie = <<Heimdall.Constants.server_cookie()::64>>

          {:cookie,
           %EDNS.Cookie{
             client_cookie: client_cookie,
             server_cookie: server_cookie,
             opt_length: byte_size(client_cookie) + byte_size(server_cookie)
           }}

        option ->
          option
      end)

    new_rdata = %Model.EDNS{additional.rdata | options: updated_options}

    %Model.ResourceRecord{additional | rdata: new_rdata}
  end

  defp process_additional(additional), do: additional
end
