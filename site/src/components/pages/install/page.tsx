import { MotionConfig } from "motion/react";
import { Navbar } from "@/components/sections/navbar";
import { Footer } from "@/components/sections/footer";
import { InstallMethods } from "@/components/sections/install-methods";
import { LoginItemsApproval } from "@/components/sections/login-items-approval";
import { UpdateUninstall } from "@/components/sections/update-uninstall";
import { InstallHero } from "./sections/install-hero";

/**
 * @ployComponent
 * @ployComponentId OpenLidInstallPage
 * @ployComponentType page
 * @ployComponentPattern landing
 * @ployComponentStatus stable
 * @ployComponentDescription Dedicated /install page shell. Composes the shared
 * Navbar + Footer with the install reference sections in order: InstallHero,
 * InstallMethods, LoginItemsApproval, UpdateUninstall. These were lifted off the
 * homepage so the home scroll stays a focused pitch while install/setup/upkeep
 * gets its own canonical home. Wrapped in MotionConfig (reducedMotion="user")
 * so the section scroll reveals respect accessibility, matching HomePage.
 */
export function InstallPage() {
  return (
    <MotionConfig reducedMotion="user">
      <div className="min-h-screen bg-ploy-background-primary text-ploy-text-primary">
        <Navbar />
        <main>
          <InstallHero />
          <InstallMethods />
          <LoginItemsApproval />
          <UpdateUninstall />
        </main>
        <Footer />
      </div>
    </MotionConfig>
  );
}
