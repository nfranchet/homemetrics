# Kubernetes deployment

The `k8s/` directory is a Kustomize base for deploying HomeMetrics with Argo CD. It deploys one daemon pod and expects an existing PostgreSQL/TimescaleDB database and External Secrets Operator.

## Configure the cluster and secrets

1. Ensure External Secrets Operator is installed and create a `ClusterSecretStore` named `homemetrics-secrets`, or change `secretStoreRef` in `external-secret.yaml` to the store already used by your cluster.
2. Add a remote secret named `homemetrics/production` with these properties:

   | Property | Value |
   | --- | --- |
   | `gmail-credentials-json` | OAuth client credentials JSON |
   | `gmail-token-cache-json` | Pre-authorized token cache JSON |
   | `db-password` | Database password |
   | `slack-bot-token` | Optional Slack bot token |
   | `slack-channel-id` | Optional Slack channel ID |

   The pod copies the initial token cache to its persistent volume only when no cache exists there. Token refreshes then update the persistent copy. Complete the OAuth browser authorization before storing the initial cache; the pod cannot perform that interactive first login.
3. Set the database host, name, username, and schedule in `configmap.yaml`. The database must already have the schema from `init_db.sql` and the TimescaleDB extension enabled.
4. Ensure the cluster can pull `ghcr.io/nfranchet/homemetrics`. Make the GHCR package public, or configure an `imagePullSecret` in the pod spec for a private package. The persistent volume claim uses the `local-path` StorageClass and requests 1 GiB.

## Connect Argo CD

Add an `Application` like this in the Argo CD repository, adjusting the project and source revision to match your setup:

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: homemetrics
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/nfranchet/homemetrics.git
    targetRevision: main
    path: k8s
    kustomize:
      images:
        - "ghcr.io/nfranchet/homemetrics=ghcr.io/nfranchet/homemetrics:<commit-sha>"
  destination:
    server: https://kubernetes.default.svc
    namespace: homemetrics
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
      - CreateNamespace=true
```

The GitHub Actions workflow publishes each pushed commit as `ghcr.io/nfranchet/homemetrics:<commit-sha>` and also publishes the compatibility tag `0.0.1`. Pin the desired SHA in the Argo CD repository's Kustomize image override, for example:

```yaml
spec:
  source:
    kustomize:
      images:
        - "ghcr.io/nfranchet/homemetrics=ghcr.io/nfranchet/homemetrics:<commit-sha>"
```

Update that override to the new commit SHA when you want Argo CD to deploy a newer build. Pull requests build the image but do not publish it.

## State and scaling

Keep `replicas: 1`: this worker processes a mailbox and should not run competing copies. The `Recreate` update strategy avoids overlapping pods while they use the same ReadWriteOnce volume. The volume persists the Gmail token cache and attachment data across pod restarts.
