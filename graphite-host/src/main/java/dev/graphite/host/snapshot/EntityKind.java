package dev.graphite.host.snapshot;

import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.Projectile;

public enum EntityKind {
    UNKNOWN(0),
    PLAYER(1),
    MOB(2),
    ITEM(3),
    PROJECTILE(4);

    public final short id;

    EntityKind(int id) {
        this.id = (short) id;
    }

    public static short of(Entity entity) {
        if (entity instanceof Player) {
            return PLAYER.id;
        }
        if (entity instanceof Mob) {
            return MOB.id;
        }
        if (entity instanceof ItemEntity) {
            return ITEM.id;
        }
        if (entity instanceof Projectile) {
            return PROJECTILE.id;
        }
        return UNKNOWN.id;
    }
}
