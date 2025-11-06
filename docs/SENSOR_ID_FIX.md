# Fix Sensor ID et Location pour X-Sense

## Problème Identifié

Les métriques Prometheus affichaient des valeurs incorrectes pour `sensor_id` et `location` :

```
❌ AVANT :
avg_temperature{location="Bureau_Exporter les données_20251031", sensor_id="Bureau_Exporter les données_20251031"}
avg_temperature{location="Bureau_Exporter les données_20251101", sensor_id="Bureau_Exporter les données_20251101"}
```

Au lieu des valeurs attendues :
```
✅ ATTENDU :
avg_temperature{location="Bureau", sensor_id="Bureau"}
avg_temperature{location="Salon", sensor_id="Salon"}
```

## Cause Racine

La fonction `extract_sensor_name()` dans `src/xsense/extractor.rs` utilisait un regex qui ne matchait que le format avec préfixe "Thermo-" :

```rust
// ❌ ANCIEN CODE - Ne supporte qu'un seul format
if let Some(captures) = Regex::new(r"Thermo-([^_]+)_")?.captures(filename) {
    if let Some(sensor_match) = captures.get(1) {
        return Ok(sensor_match.as_str().to_string());
    }
}

// Fallback problématique : retourne le filename complet !
let name = filename.split('.').next().unwrap_or(filename);
Ok(name.to_string())
```

**Le problème** : Quand le filename ne commence pas par "Thermo-", le fallback retournait le filename complet sans extension, ce qui donnait "Bureau_Exporter les données_20251031".

## Formats de Fichiers Détectés

Deux formats différents sont utilisés par X-Sense selon la configuration :

### Format 1 : Avec préfixe "Thermo-" (ancien format)
```
Thermo-cabane_Exporter les données_20251104.csv
Thermo-patio_Exporter les données_20251105.csv
Thermo-poolhouse_Export data_20251106.csv
```
→ Extrait : "cabane", "patio", "poolhouse"

### Format 2 : Sans préfixe (nouveau format français)
```
Bureau_Exporter les données_20251031.csv
Salon_Exporter les données_20251101.csv
Cuisine_Export data_20251102.csv
```
→ Devrait extraire : "Bureau", "Salon", "Cuisine"

## Solution Implémentée

Mise à jour de `extract_sensor_name()` pour supporter les deux formats avec deux regex :

```rust
// ✅ NOUVEAU CODE - Support de plusieurs formats
pub fn extract_sensor_name(filename: &str) -> Result<String> {
    // Try format with "Thermo-" prefix first
    if let Some(captures) = Regex::new(r"Thermo-([^_]+)_")?.captures(filename) {
        if let Some(sensor_match) = captures.get(1) {
            return Ok(sensor_match.as_str().to_string());
        }
    }
    
    // Try format without "Thermo-" prefix: extract first part before underscore
    // Example: "Bureau_Exporter les données_20251031.csv" -> "Bureau"
    if let Some(captures) = Regex::new(r"^([^_]+)_(?:Exporter|Export)")?.captures(filename) {
        if let Some(sensor_match) = captures.get(1) {
            return Ok(sensor_match.as_str().to_string());
        }
    }
    
    // Fallback: use full filename without extension (unchanged)
    let name = filename.split('.').next().unwrap_or(filename);
    Ok(name.to_string())
}
```

### Logique de Détection

1. **Regex 1** : `r"Thermo-([^_]+)_"`
   - Cherche "Thermo-" au début
   - Capture tout jusqu'au premier "_"
   - Exemple : "Thermo-cabane_..." → "cabane"

2. **Regex 2** : `r"^([^_]+)_(?:Exporter|Export)"`
   - Cherche depuis le début du filename (`^`)
   - Capture tout jusqu'au premier "_" 
   - Vérifie que ça soit suivi de "Exporter" ou "Export"
   - Exemple : "Bureau_Exporter les données_..." → "Bureau"

3. **Fallback** : Si aucun pattern ne matche, utilise le filename sans extension

## Tests Ajoutés

Mise à jour de `tests/xsense_test.rs` avec les nouveaux cas :

```rust
#[test]
fn test_extract_sensor_name() {
    // Format with "Thermo-" prefix
    assert_eq!(
        TemperatureExtractor::extract_sensor_name("Thermo-cabane_Exporter les données_20251104.csv").unwrap(),
        "cabane"
    );
    
    // Format without "Thermo-" prefix (French format)
    assert_eq!(
        TemperatureExtractor::extract_sensor_name("Bureau_Exporter les données_20251031.csv").unwrap(),
        "Bureau"
    );
    
    // English format variation
    assert_eq!(
        TemperatureExtractor::extract_sensor_name("Kitchen_Export data_20251103.csv").unwrap(),
        "Kitchen"
    );
}
```

## Flux de Données Corrigé

### Avant (INCORRECT)
```
Filename: "Bureau_Exporter les données_20251031.csv"
    ↓ extract_sensor_name()
    ↓ [Regex 1 échoue - pas de "Thermo-"]
    ↓ [Fallback - retire .csv]
sensor_name = "Bureau_Exporter les données_20251031"
    ↓
TemperatureReading {
    sensor_id: "Bureau_Exporter les données_20251031",  ❌
    location: Some("Bureau_Exporter les données_20251031"),  ❌
    ...
}
```

### Après (CORRECT)
```
Filename: "Bureau_Exporter les données_20251031.csv"
    ↓ extract_sensor_name()
    ↓ [Regex 1 échoue - pas de "Thermo-"]
    ↓ [Regex 2 réussit - "^([^_]+)_(?:Exporter|Export)"]
    ↓ [Capture "Bureau"]
sensor_name = "Bureau"  ✅
    ↓
TemperatureReading {
    sensor_id: "Bureau",  ✅
    location: Some("Bureau"),  ✅
    temperature: 20.5,
    humidity: Some(65.0),
    timestamp: ...
}
```

## Impact sur la Base de Données

### Données Existantes (Polluées)

Les anciennes données ont des sensor_id incorrects :
```sql
SELECT DISTINCT sensor_id FROM temperature_readings 
WHERE sensor_id LIKE '%Exporter%';

-- Résultat :
-- "Bureau_Exporter les données_20251031"
-- "Bureau_Exporter les données_20251101"
-- "Salon_Exporter les données_20251102"
```

### Nettoyage Recommandé

```sql
-- Option 1 : Supprimer les anciennes données incorrectes
DELETE FROM temperature_readings 
WHERE sensor_id LIKE '%Exporter%';

-- Option 2 : Corriger les sensor_id existants (plus complexe)
UPDATE temperature_readings
SET sensor_id = SPLIT_PART(sensor_id, '_', 1),
    location = SPLIT_PART(sensor_id, '_', 1)
WHERE sensor_id LIKE '%Exporter%';
```

**Recommandation** : Supprimer les données incorrectes car elles ont aussi des timestamps potentiellement dupliqués (un fichier par jour avec date dans le nom).

### Nouvelles Données (Correctes)

Après le fix, toutes les nouvelles données auront :
```sql
SELECT sensor_id, location, COUNT(*) 
FROM temperature_readings 
GROUP BY sensor_id, location;

-- Résultat attendu :
-- Bureau      | Bureau      | 1440  (1 jour * 1 mesure/min)
-- Salon       | Salon       | 1440
-- Cuisine     | Cuisine     | 1440
```

## Impact sur Prometheus

### Métriques AVANT (incorrectes)
```promql
# Chaque export de fichier créait une nouvelle série temporelle !
avg_temperature{location="Bureau_Exporter les données_20251031", sensor_id="Bureau_Exporter les données_20251031"}
avg_temperature{location="Bureau_Exporter les données_20251101", sensor_id="Bureau_Exporter les données_20251101"}
avg_temperature{location="Bureau_Exporter les données_20251102", sensor_id="Bureau_Exporter les données_20251102"}
```

**Problème** : Impossibilité d'agréger les données du même capteur sur plusieurs jours !

### Métriques APRÈS (correctes)
```promql
# Une seule série temporelle par capteur
avg_temperature{location="Bureau", sensor_id="Bureau"} 20.5
avg_temperature{location="Salon", sensor_id="Salon"} 22.3
avg_temperature{location="Cuisine", sensor_id="Cuisine"} 19.8
```

**Avantage** : Queries Prometheus fonctionnelles :
```promql
# Évolution de la température du Bureau sur 7 jours
avg_temperature{sensor_id="Bureau"}[7d]

# Comparaison multi-capteurs
sum by (sensor_id) (avg_temperature)
```

## Fichiers Modifiés

1. **`src/xsense/extractor.rs`** :
   - Fonction `extract_sensor_name()` : Ajout regex pour format sans "Thermo-"
   - Documentation mise à jour avec exemples des deux formats

2. **`src/xsense/mod.rs`** :
   - Export de `TemperatureExtractor` pour les tests
   - Ajout : `pub use extractor::TemperatureExtractor;`

3. **`tests/xsense_test.rs`** :
   - Tests étendus pour couvrir les deux formats de filenames
   - Ajout de cas de test pour "Bureau_Exporter...", "Salon_Exporter...", etc.

## Validation

✅ **Tests unitaires** : `cargo test test_extract_sensor_name` - PASS
✅ **Compilation** : `cargo build --release` - SUCCESS
✅ **Couverture** : Les deux formats supportés
✅ **Rétrocompatibilité** : Le format "Thermo-" continue de fonctionner

## Prochaines Étapes

1. **Nettoyer la base de données** :
   ```bash
   psql -h localhost -U homemetrics_user -d homemetrics_db -c \
     "DELETE FROM temperature_readings WHERE sensor_id LIKE '%Exporter%';"
   ```

2. **Retraiter les emails** (optionnel) :
   - Marquer les emails "done" comme "todo" dans Gmail
   - Relancer le traitement : `cargo run --release`

3. **Vérifier Prometheus** :
   ```promql
   # Vérifier qu'il n'y a plus de séries avec "Exporter"
   count by (sensor_id) (avg_temperature)
   ```

4. **Surveillance** :
   - Confirmer que les nouveaux sensor_id sont courts et cohérents
   - Vérifier que les requêtes d'agrégation fonctionnent

## Notes Techniques

- Le regex utilise `(?:Exporter|Export)` pour supporter les deux langues (non-capturing group)
- Le `^` au début du regex 2 assure qu'on capture depuis le début du filename
- `[^_]+` capture tout sauf underscore (greedy jusqu'au premier `_`)
- La priorité des regex garantit la rétrocompatibilité (Thermo- testé en premier)
