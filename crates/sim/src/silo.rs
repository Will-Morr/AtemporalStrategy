//! Missile inventories, queued/automatic launch and deterministic delayed impacts.
use crate::world::Sim;
use crate::*;

impl Sim {
    pub(crate) fn silo_plan_valid(&self, key: &str, plan: &SiloPlan) -> bool {
        self.type_index(key).is_ok_and(|i| {
            validate_silo_plan(
                &self.content.types[i],
                plan,
                self.state.terrain.width,
                self.state.terrain.height,
            )
            .is_ok()
        })
    }

    /// Consuming an item advances its identity counter and returns only that item's repeat flag.
    pub(crate) fn finish_production(&mut self, index: usize) {
        let prod = self.state.entities[index].production.as_mut().unwrap();
        let active = prod.active_item.take().unwrap();
        match prod
            .occurrence_counters
            .iter_mut()
            .find(|c| c.item_id == active.item_id)
        {
            Some(c) => c.next_occurrence = active.occurrence + 1,
            None => prod.occurrence_counters.push(OccurrenceCounter {
                item_id: active.item_id.clone(),
                next_occurrence: active.occurrence + 1,
            }),
        }
        if active.loop_enabled {
            prod.pending_items.push(QueueItem {
                item_id: active.item_id,
                type_key: active.type_key,
                loop_enabled: true,
            });
        }
    }

    pub(crate) fn stock_missiles(&mut self) -> Result<()> {
        for i in 0..self.state.entities.len() {
            let entity = &self.state.entities[i];
            if entity.lifecycle != Lifecycle::Complete || self.def(i).silo.is_none() {
                continue;
            }
            let Some(active) = entity
                .production
                .as_ref()
                .and_then(|p| p.active_item.as_ref())
                .filter(|a| a.awaiting_output)
            else {
                continue;
            };
            let key = active.type_key.clone();
            let owner = entity.owner;
            let silo = self.state.entities[i]
                .production
                .as_mut()
                .and_then(|p| p.silo.as_mut())
                .ok_or("silo missing inventory")?;
            if let Some(stock) = silo.inventory.iter_mut().find(|s| s.type_key == key) {
                stock.count = stock
                    .count
                    .checked_add(1)
                    .ok_or("missile inventory overflow")?;
            } else {
                silo.inventory.push(MissileStock {
                    type_key: key,
                    count: 1,
                });
                silo.inventory.sort_by(|a, b| a.type_key.cmp(&b.type_key));
            }
            self.finish_production(i);
            self.progress = true;
            self.note(owner, Activity::Construction);
        }
        Ok(())
    }

    /// Normal allied vision plus moving satellites and their temporary destination coverage.
    fn side_sees(&self, owner: PlayerId, tile: Tile) -> bool {
        let within = |x: f64, y: f64, r: f64| {
            (x - f64::from(tile.x)).powi(2) + (y - f64::from(tile.y)).powi(2) <= r * r + 1e-9
        };
        self.state.entities.iter().enumerate().any(|(i, e)| {
            e.lifecycle == Lifecycle::Complete
                && !self.hostile(owner, e.owner)
                && within(f64::from(e.tile.x), f64::from(e.tile.y), self.def(i).vision)
        }) || self.state.recon.iter().any(|r| {
            r.expires_at > self.state.tick
                && !self.hostile(owner, r.owner)
                && within(f64::from(r.center.x), f64::from(r.center.y), r.radius)
        }) || self.state.missiles.iter().any(|f| {
            !self.hostile(owner, f.owner)
                && f.launch_tick < self.state.tick
                && self.type_index(&f.type_key).is_ok_and(|i| {
                    self.content.types[i].missile.as_ref().is_some_and(|m| {
                        let (x, y) = atemporal_contracts::missiles::flight_position(
                            f,
                            f64::from(self.state.tick),
                        );
                        m.effect == MissileEffect::Satellite && within(x, y, m.radius)
                    })
                })
        })
    }

    pub(crate) fn launch_missiles(&mut self) -> Result<()> {
        let tick = self.state.tick;
        for i in 0..self.state.entities.len() {
            let entity = &self.state.entities[i];
            let Some(cap) = self.def(i).silo.clone() else {
                continue;
            };
            if entity.lifecycle != Lifecycle::Complete {
                continue;
            }
            let Some(silo) = entity.production.as_ref().and_then(|p| p.silo.as_ref()) else {
                continue;
            };
            if silo.next_launch_tick > tick {
                continue;
            }
            let manual = !silo.plan.launches.is_empty();
            let order = if let Some(order) = silo.plan.launches.first() {
                Some(order.clone())
            } else if silo.plan.automatic {
                let target = self
                    .state
                    .entities
                    .iter()
                    .filter(|other| {
                        self.hostile(entity.owner, other.owner)
                            && Self::dist2(entity.tile, other.tile)
                                <= cap.auto_range * cap.auto_range
                            && self.side_sees(entity.owner, other.tile)
                    })
                    .min_by(|a, b| {
                        Self::dist2(entity.tile, a.tile)
                            .total_cmp(&Self::dist2(entity.tile, b.tile))
                            .then(a.id.cmp(&b.id))
                    });
                target.and_then(|target| {
                    silo.inventory
                        .iter()
                        .find(|s| s.count > 0)
                        .map(|s| MissileLaunch {
                            type_key: s.type_key.clone(),
                            target: target.tile,
                        })
                })
            } else {
                None
            };
            let Some(order) = order else {
                continue;
            };
            if !silo
                .inventory
                .iter()
                .any(|s| s.type_key == order.type_key && s.count > 0)
            {
                continue;
            }
            let missile = self.content.types[self.type_index(&order.type_key)?]
                .missile
                .as_ref()
                .ok_or("invalid missile recipe")?;
            let flight = MissileFlight {
                silo_id: entity.id.clone(),
                sequence: silo.sequence,
                owner: entity.owner,
                type_key: order.type_key.clone(),
                origin: entity.tile,
                target: order.target,
                launch_tick: tick,
                impact_tick: tick
                    + atemporal_contracts::missiles::flight_ticks(
                        entity.tile,
                        order.target,
                        missile,
                    ),
            };
            let silo = self.state.entities[i]
                .production
                .as_mut()
                .unwrap()
                .silo
                .as_mut()
                .unwrap();
            silo.inventory
                .iter_mut()
                .find(|s| s.type_key == order.type_key)
                .unwrap()
                .count -= 1;
            silo.sequence = silo
                .sequence
                .checked_add(1)
                .ok_or("missile sequence overflow")?;
            silo.next_launch_tick = tick + cap.launch_cooldown;
            if manual {
                silo.plan.launches.remove(0);
            }
            self.state.missiles.push(flight.clone());
            self.progress = true;
            self.note(flight.owner, Activity::Combat);
            self.push_event(PresentationEvent::MissileLaunch { flight });
        }
        Ok(())
    }

    pub(crate) fn missile_impacts(&mut self, damage: &mut [f64]) -> Result<Vec<bool>> {
        let mut annihilated = vec![false; damage.len()];
        let tick = self.state.tick;
        self.state.recon.retain(|r| r.expires_at > tick);
        let due: Vec<_> = self
            .state
            .missiles
            .iter()
            .filter(|f| f.impact_tick <= tick)
            .cloned()
            .collect();
        self.state.missiles.retain(|f| f.impact_tick > tick);
        for flight in due {
            let missile = self.content.types[self.type_index(&flight.type_key)?]
                .missile
                .clone()
                .ok_or("flight references non-missile")?;
            if missile.effect == MissileEffect::Satellite {
                if missile.reveal_ticks > 0 {
                    self.state.recon.push(ReconZone {
                        starts_at: tick + 1,
                        owner: flight.owner,
                        center: flight.target,
                        radius: missile.radius,
                        expires_at: tick + 1 + missile.reveal_ticks,
                    });
                }
            } else {
                for (i, dealt) in damage.iter_mut().enumerate() {
                    let e = &self.state.entities[i];
                    if Self::dist2(e.tile, flight.target) <= missile.radius * missile.radius + 1e-9
                    {
                        let amount = if missile.effect == MissileEffect::TacNuke {
                            annihilated[i] = true;
                            e.hp
                        } else {
                            missile.damage
                        };
                        *dealt += amount;
                        self.state.players[usize::from(flight.owner)]
                            .counters
                            .damage_dealt += amount.min(e.hp);
                        let owner = e.owner;
                        self.note(owner, Activity::Combat);
                    }
                }
            }
            self.progress = true;
            self.note(flight.owner, Activity::Combat);
            self.push_event(PresentationEvent::MissileImpact {
                owner: flight.owner,
                type_key: flight.type_key,
                target: flight.target,
                radius: missile.radius,
            });
        }
        Ok(annihilated)
    }
}

impl Sim {
    /// A scheduled launch is meaningful work even when ordinary unit orders are idle.
    pub(crate) fn silo_has_launch_work(&self, i: usize, include_production: bool) -> bool {
        let e = &self.state.entities[i];
        let Some(cap) = &self.def(i).silo else {
            return false;
        };
        let Some(p) = &e.production else {
            return false;
        };
        let Some(silo) = &p.silo else {
            return false;
        };
        let available = |key: Option<&str>| {
            silo.inventory
                .iter()
                .any(|s| s.count > 0 && key.is_none_or(|k| s.type_key == k))
                || (include_production
                    && e.priority != Priority::Off
                    && self.state.players[usize::from(e.owner)].bank > 0.0
                    && self
                        .def(i)
                        .production
                        .as_ref()
                        .is_some_and(|p| p.rate > 0.0)
                    && (p
                        .active_item
                        .as_ref()
                        .is_some_and(|a| key.is_none_or(|k| a.type_key == k))
                        || p.pending_items
                            .iter()
                            .any(|a| key.is_none_or(|k| a.type_key == k))))
        };
        if let Some(order) = silo.plan.launches.first() {
            return available(Some(&order.type_key));
        }
        silo.plan.automatic
            && available(None)
            && self.state.entities.iter().any(|target| {
                self.hostile(e.owner, target.owner)
                    && Self::dist2(e.tile, target.tile) <= cap.auto_range * cap.auto_range
                    && self.side_sees(e.owner, target.tile)
            })
    }
}
