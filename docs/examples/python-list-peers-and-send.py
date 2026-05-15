from localsend_improved import Client

client = Client(token_from_file=True)
peers = client.peers.list_trusted()
print(f"trusted peers: {len(peers)}")
result = client.transfers.send(paths=["./photo.jpg"], peer_fingerprint=peers[0]["fingerprint"])
print(result["transfer_id"])
